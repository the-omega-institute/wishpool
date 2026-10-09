use super::{App, clamp};
use crate::{
    CoreError, CoreResult,
    ids::PersonId,
    model::{Caller, Person, Role, VerifiedIdentity},
    ports::Listing,
};

impl App {
    /// Record a completed sign-in and return the person.
    pub async fn sign_in(&self, identity: &VerifiedIdentity) -> CoreResult<Person> {
        let now = self.ports.clock.now();
        let person = self.ports.people.upsert_sign_in(identity, now).await?;
        if self.bootstrap_admins.contains(&person.id) && !person.has(Role::Admin) {
            let mut roles: Vec<Role> = person.roles.iter().copied().collect();
            roles.push(Role::Admin);
            return self.ports.people.set_roles(&person.id, &roles).await;
        }
        Ok(person)
    }

    /// Resolve the caller for an authenticated subject. A subject that has
    /// never signed in through the browser flow (a machine account presenting
    /// a bearer token) is created on first use.
    pub async fn caller(&self, identity: &VerifiedIdentity) -> CoreResult<Caller> {
        let person = match self.ports.people.get(&identity.subject).await? {
            Some(person) => person,
            None => self.sign_in(identity).await?,
        };
        Ok(Caller::from_person(&person))
    }

    /// Ensure a deployment-owned service account exists with `role`, and
    /// return its caller. Composition-only: no interface projects this.
    pub async fn ensure_service_account(
        &self,
        subject: &PersonId,
        name: &str,
        role: Role,
    ) -> CoreResult<Caller> {
        let identity = VerifiedIdentity {
            subject: subject.clone(),
            name: Some(name.to_owned()),
            email: None,
            picture: None,
        };
        let mut person = self
            .ports
            .people
            .upsert_sign_in(&identity, self.ports.clock.now())
            .await?;
        if !person.has(role) {
            let mut roles: Vec<Role> = person.roles.iter().copied().collect();
            roles.push(role);
            person = self.ports.people.set_roles(subject, &roles).await?;
        }
        Ok(Caller::from_person(&person))
    }

    pub async fn me(&self, caller: &Caller) -> CoreResult<Person> {
        self.ports
            .people
            .get(&caller.person)
            .await?
            .ok_or_else(|| CoreError::not_found("person", caller.person.as_str()))
    }

    pub async fn person_name(&self, id: &PersonId) -> CoreResult<String> {
        Ok(self
            .ports
            .people
            .get(id)
            .await?
            .map(|p| p.display_name)
            .unwrap_or_else(|| id.to_string()))
    }

    pub async fn list_people(
        &self,
        caller: &Caller,
        limit: Option<u32>,
        before: Option<String>,
    ) -> CoreResult<Listing<Person>> {
        caller.require(Role::Admin)?;
        self.ports.people.list(clamp(limit), before).await
    }

    pub async fn set_roles(
        &self,
        caller: &Caller,
        id: &PersonId,
        roles: &[Role],
    ) -> CoreResult<Person> {
        caller.require(Role::Admin)?;
        if id == &caller.person && !roles.contains(&Role::Admin) {
            return Err(CoreError::conflict(
                "an administrator cannot remove their own Admin role",
            ));
        }
        if self.ports.people.get(id).await?.is_none() {
            return Err(CoreError::not_found("person", id.as_str()));
        }
        self.ports.people.set_roles(id, roles).await
    }
}
