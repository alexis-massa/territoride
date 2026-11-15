/** A path-based router, enough for six pages: the current path, and `navigate` for in-app links. */
class Router {
  path = $state(window.location.pathname);

  constructor() {
    window.addEventListener('popstate', () => (this.path = window.location.pathname));
  }

  navigate(to: string) {
    if (to === window.location.pathname + window.location.search) return;
    window.history.pushState({}, '', to);
    this.path = window.location.pathname;
    window.scrollTo(0, 0);
  }
}

export const router = new Router();
