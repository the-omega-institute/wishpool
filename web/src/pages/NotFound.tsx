import { Link } from '../routing/router';

export function NotFoundPage({ path }: { path: string }) {
  return (
    <div className="page narrow-page">
      <h1>Page not found</h1>
      <p>
        Nothing lives at <code>{path}</code>.
      </p>
      <p>
        <Link to={{ kind: 'home' }}>Return to the front page</Link>
      </p>
    </div>
  );
}
