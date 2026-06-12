// Loads cookies from a Netscape-format cookies.txt file.
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));

// Resolution order: explicit arg > $EDUCODER_COOKIES > ./cookies.txt (project root)
export function resolveCookiesPath(explicit) {
  const candidates = [
    explicit,
    process.env.EDUCODER_COOKIES,
    path.join(__dirname, '..', 'cookies.txt'),
  ].filter(Boolean);
  for (const c of candidates) {
    if (fs.existsSync(c)) return c;
  }
  throw new Error(
    'No cookies file found. Set $EDUCODER_COOKIES, drop a cookies.txt in the project root, ' +
    'or pass a path. Tried:\n  ' + candidates.join('\n  ')
  );
}

// Parses a Netscape cookies.txt into a { name: value } map.
export function loadCookies(explicit) {
  const file = resolveCookiesPath(explicit);
  const txt = fs.readFileSync(file, 'utf8');
  const cookies = {};
  for (const line of txt.split('\n')) {
    if (!line.trim() || line.startsWith('#')) continue;
    const parts = line.split('\t');
    if (parts.length >= 7) cookies[parts[5]] = parts[6].trim();
  }
  if (!cookies._educoder_session) {
    throw new Error(`No _educoder_session cookie in ${file} (logged out or expired?)`);
  }
  return { cookies, file };
}
