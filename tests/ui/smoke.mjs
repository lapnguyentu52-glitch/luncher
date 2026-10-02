/**
 * UI smoke test — spec 3.0 mục 46 (tests/ui/).
 *
 * Kiểm các phần frontend KHÔNG cần DOM thật:
 *   1. i18n: en/vi parse được, bộ key giống hệt nhau, không value rỗng,
 *      placeholder {x} đồng bộ giữa 2 ngôn ngữ.
 *   2. Router: mọi route có view + titleKey tồn tại trong CẢ en và vi;
 *      NAV_GROUPS <-> ROUTES khớp 2 chiều (không route mồ côi / nav ảo).
 *   3. Module import: mọi file JS trong app/ và tabs/ (đệ quy) import được
 *      trong Node headless — bắt lỗi syntax/top-level DOM sớm.
 *
 * Chạy:  node tests/ui/smoke.mjs   (từ antares-src/)
 * Thoát: exit 0 = pass, 1 = fail (liệt kê từng lỗi).
 */
import fs from 'node:fs';
import path from 'node:path';

// tests/ui/ -> ../.. = antares-src/
const ROOT = path.resolve(new URL('../..', import.meta.url).pathname);
let failures = [];
let checks = 0;

function check(cond, label) {
  checks++;
  if (!cond) failures.push(label);
}

/* ------------------------------------------------------------------ */
/* 1. i18n — en/vi parity (mục 46 "Language")                          */
/* ------------------------------------------------------------------ */

const en = JSON.parse(fs.readFileSync(path.join(ROOT, 'frontend/i18n/en.json'), 'utf8'));
const vi = JSON.parse(fs.readFileSync(path.join(ROOT, 'frontend/i18n/vi.json'), 'utf8'));
check(Object.keys(en).length > 100, `i18n/en.json có dữ liệu (${Object.keys(en).length} keys)`);

const enKeys = new Set(Object.keys(en));
const viKeys = new Set(Object.keys(vi));
const missingInVi = [...enKeys].filter((k) => !viKeys.has(k));
const missingInEn = [...viKeys].filter((k) => !enKeys.has(k));
check(missingInVi.length === 0, `key thiếu trong vi.json: ${missingInVi.slice(0, 10).join(', ') || '—'}`);
check(missingInEn.length === 0, `key thiếu trong en.json: ${missingInEn.slice(0, 10).join(', ') || '—'}`);

const emptyValues = Object.entries({ ...en, ...vi })
  .filter(([, v]) => typeof v !== 'string' || v.trim() === '')
  .map(([k]) => k);
check(emptyValues.length === 0, `value rỗng: ${emptyValues.slice(0, 10).join(', ') || '—'}`);

// placeholder {x} phải đồng bộ 2 chiều (mục 578: không nối chuỗi raw)
const PLACEHOLDER_RE = /\{(\w+)\}/g;
const phProblems = [];
for (const key of enKeys) {
  if (!viKeys.has(key)) continue;
  const ph = (s) => new Set([...String(s).matchAll(PLACEHOLDER_RE)].map((m) => m[1]));
  const a = ph(en[key]);
  const b = ph(vi[key]);
  const diff = [...a].filter((x) => !b.has(x)).concat([...b].filter((x) => !a.has(x)));
  if (diff.length) phProblems.push(`${key}: {${diff.join('}, {')}}`);
}
check(phProblems.length === 0, `placeholder lệch: ${phProblems.slice(0, 5).join(' | ') || '—'}`);

/* ------------------------------------------------------------------ */
/* 2. Router registry — route <-> nav 2 chiều (mục 46 "Router")         */
/* ------------------------------------------------------------------ */

const router = await import('../../frontend/app/router.js');
const { ROUTES, NAV_GROUPS, routeList, DEFAULT_ROUTE, isValidRoute } = router;

check(Object.keys(ROUTES).length >= 18, `số route: ${Object.keys(ROUTES).length}`);
check(isValidRoute(DEFAULT_ROUTE), 'DEFAULT_ROUTE hợp lệ');
check(routeList().length === Object.keys(ROUTES).length, 'mọi route đều nằm trong 1 group nav');

// nav group -> route tồn tại, không trùng
const navIds = NAV_GROUPS.flatMap((g) => g.items);
check(new Set(navIds).size === navIds.length, `nav item trùng: ${navIds.filter((v, i) => navIds.indexOf(v) !== i).join(', ') || '—'}`);
check(navIds.every((id) => id in ROUTES), 'nav item không có trong ROUTES');

// route -> titleKey có trong i18n
const badTitle = Object.keys(ROUTES).filter((id) => !(ROUTES[id].titleKey in en));
check(badTitle.length === 0, `titleKey thiếu trong i18n: ${badTitle.join(', ') || '—'}`);

// route -> view function
const badView = Object.keys(ROUTES).filter((id) => typeof ROUTES[id].view !== 'function');
check(badView.length === 0, `view không phải function: ${badView.join(', ') || '—'}`);

/* ------------------------------------------------------------------ */
/* 3. Module import headless (mục 46 "App boot" subset)                 */
/* ------------------------------------------------------------------ */

const appDir = path.join(ROOT, 'frontend/app');
for (const f of fs.readdirSync(appDir).filter((f) => f.endsWith('.js'))) {
  try {
    await import(`../../frontend/app/${f}`);
    checks++;
  } catch (e) {
    failures.push(`import app/${f}: ${e.message.split('\n')[0]}`);
  }
}

const tabsDir = path.join(ROOT, 'frontend/tabs');
const tabFiles = fs.readdirSync(tabsDir, { recursive: true })
  .map((f) => String(f).replaceAll('\\', '/'))
  .filter((f) => f.endsWith('.js'));
for (const f of tabFiles) {
  try {
    await import(`../../frontend/tabs/${f}`);
    checks++;
  } catch (e) {
    failures.push(`import tabs/${f}: ${e.message.split('\n')[0]}`);
  }
}

/* ------------------------------------------------------------------ */
/* Báo cáo                                                              */
/* ------------------------------------------------------------------ */

const total = checks + failures.length;
console.log(`UI smoke (mục 46): ${checks}/${total} checks passed`);
if (failures.length) {
  console.log('FAILURES:');
  for (const f of failures) console.log('  -', f);
  process.exit(1);
}
console.log('UI_SMOKE_OK');
