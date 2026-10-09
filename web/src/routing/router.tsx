import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useState,
  type AnchorHTMLAttributes,
  type MouseEvent,
  type ReactNode,
} from 'react';
import { parseRoute, routePath, type Route } from './routes';

interface RouterValue {
  route: Route;
  navigate: (to: Route | string, options?: { replace?: boolean }) => void;
}

const RouterContext = createContext<RouterValue | null>(null);

function currentRoute(): Route {
  return parseRoute(window.location.pathname, window.location.search);
}

/**
 * History-API routing. Deep links work in production because nginx falls
 * back to index.html for unknown paths (`try_files $uri /index.html`).
 */
export function RouterProvider({ children }: { children: ReactNode }) {
  const [route, setRoute] = useState<Route>(currentRoute);

  useEffect(() => {
    const onPop = () => setRoute(currentRoute());
    window.addEventListener('popstate', onPop);
    return () => window.removeEventListener('popstate', onPop);
  }, []);

  const navigate = useCallback<RouterValue['navigate']>((to, options) => {
    const path = typeof to === 'string' ? to : routePath(to);
    if (options?.replace) window.history.replaceState(null, '', path);
    else window.history.pushState(null, '', path);
    setRoute(currentRoute());
    window.scrollTo?.(0, 0);
  }, []);

  return <RouterContext.Provider value={{ route, navigate }}>{children}</RouterContext.Provider>;
}

export function useRouter(): RouterValue {
  const value = useContext(RouterContext);
  if (value === null) throw new Error('useRouter must be used inside <RouterProvider>');
  return value;
}

type LinkProps = Omit<AnchorHTMLAttributes<HTMLAnchorElement>, 'href'> & { to: Route | string };

/** An in-app link: a real `<a href>` that navigates without a reload on plain clicks. */
export function Link({ to, onClick, children, ...rest }: LinkProps) {
  // Outside a router (e.g. a component rendered on its own) this is a plain link.
  const router = useContext(RouterContext);
  const href = typeof to === 'string' ? to : routePath(to);
  const handle = (event: MouseEvent<HTMLAnchorElement>) => {
    onClick?.(event);
    if (
      router === null ||
      event.defaultPrevented ||
      event.button !== 0 ||
      event.metaKey ||
      event.ctrlKey ||
      event.shiftKey ||
      event.altKey ||
      rest.target === '_blank'
    ) {
      return;
    }
    event.preventDefault();
    router.navigate(to);
  };
  return (
    <a href={href} onClick={handle} {...rest}>
      {children as ReactNode}
    </a>
  );
}
