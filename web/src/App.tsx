import { useEffect } from 'react';
import { Header } from './components/Header';
import { ContributePage } from './pages/Contribute';
import { ContributorsPage } from './pages/Contributors';
import { HomePage } from './pages/Home';
import { NotFoundPage } from './pages/NotFound';
import { PaperPage, PapersPage } from './pages/Papers';
import { PeoplePage } from './pages/People';
import { PolicyPage } from './pages/Policy';
import { SubmissionDetailPage } from './pages/SubmissionDetail';
import { SubmissionsPage } from './pages/Submissions';
import { SubmitPage } from './pages/Submit';
import { TaskPage, TasksPage } from './pages/Tasks';
import { useRouter } from './routing/router';
import { routePath, type Route } from './routing/routes';

function titleFor(route: Route): string {
  switch (route.kind) {
    case 'home':
      return 'wishpool';
    case 'papers':
      return 'Accepted papers';
    case 'paper':
      return route.record;
    case 'policy':
      return 'Review policy';
    case 'submit':
      return 'Submit a paper';
    case 'submissions':
      return 'My papers';
    case 'queue':
      return 'Review queue';
    case 'submission':
      return 'Paper';
    case 'people':
      return 'People';
    case 'contribute':
      return 'Contribute';
    case 'tasks':
      return 'Tasks';
    case 'task':
      return 'Task';
    case 'contributors':
      return 'Contributors';
    case 'not_found':
      return 'Not found';
  }
}

function Page({ route }: { route: Route }) {
  switch (route.kind) {
    case 'home':
      return <HomePage />;
    case 'papers':
      return <PapersPage />;
    case 'paper':
      return <PaperPage key={route.record} record={route.record} />;
    case 'policy':
      return <PolicyPage />;
    case 'submit':
      return <SubmitPage />;
    case 'submissions':
      return <SubmissionsPage key="mine" scope="mine" />;
    case 'queue':
      return <SubmissionsPage key="queue" scope="queue" />;
    case 'submission':
      return <SubmissionDetailPage key={route.id} id={route.id} />;
    case 'people':
      return <PeoplePage />;
    case 'contribute':
      return <ContributePage />;
    case 'tasks':
      return (
        <TasksPage
          key={routePath(route)}
          submission={route.submission}
          initialKind={route.taskKind}
        />
      );
    case 'task':
      return <TaskPage key={route.id} id={route.id} />;
    case 'contributors':
      return <ContributorsPage />;
    case 'not_found':
      return <NotFoundPage path={route.path} />;
  }
}

export function App() {
  const { route } = useRouter();
  useEffect(() => {
    const title = titleFor(route);
    document.title = title === 'wishpool' ? 'wishpool' : `${title} · wishpool`;
  }, [route]);
  return (
    <>
      <a className="skip-link" href="#main">
        Skip to content
      </a>
      <Header />
      <main id="main">
        <Page route={route} />
      </main>
      <footer className="site-footer">
        <p>wishpool</p>
      </footer>
    </>
  );
}
