import { createApi } from 'pi-ui';

/** The logged-in user, from POST /api/login and GET /api/me. */
export interface Me {
  username: string;
  csrf_token: string;
}

export const api = createApi();
