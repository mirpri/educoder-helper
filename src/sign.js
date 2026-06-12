// EduCoder request signing.
//
// The frontend signs most API calls; unsigned/stale requests get
// {"status":-102,"message":"服务器时间与您的设备时间不匹配..."}.
// Scheme (reverse-engineered from the umi.js bundle):
//   sig = md5( base64( "method=<M>&ak=<AK>&sk=<SK>&time=<ms>" ) )
// sent as X-EDU-Signature with the matching X-EDU-Timestamp. The timestamp
// must be close to *server* time, so callers align to the server clock.
import crypto from 'node:crypto';

// ak/sk are the double-base64-decoded constants from the `_key` webpack module.
export const AK = 'e9dd5b4322f9f7d83d009de9bfa100c3';
export const SK = '2e3da06ae26ba9f76a5d8d355746f2fe';

export function signature(method, timeMs) {
  const raw = `method=${method.toUpperCase()}&ak=${AK}&sk=${SK}&time=${timeMs}`;
  const b64 = Buffer.from(raw, 'utf8').toString('base64');
  return crypto.createHash('md5').update(b64).digest('hex');
}
