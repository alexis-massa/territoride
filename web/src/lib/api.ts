import { createApi } from 'pi-ui';

import type { Outcome } from './types';

export const api = createApi();

let csrf: string | null = null;

/** Remember the session's CSRF token (also given to the shared client). */
export function setCsrf(token: string | null) {
  csrf = token;
  api.setCsrf(token);
}

/** Upload a GPX file; the shared client only sends JSON. */
export async function uploadGpx(file: File): Promise<Outcome> {
  const body = new FormData();
  body.append('gpx_file', file);
  const res = await fetch('/api/import-activity', {
    method: 'POST',
    headers: { 'X-CSRF-Token': csrf ?? '' },
    credentials: 'same-origin',
    body,
  });
  if (!res.ok)
    throw new Error(
      ((await res.json().catch(() => null)) as { detail?: string } | null)?.detail ?? 'Upload failed',
    );
  return (await res.json()) as Outcome;
}
