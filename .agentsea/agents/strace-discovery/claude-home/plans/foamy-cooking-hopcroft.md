# Plan: Scaffold Flotsam Web Frontend

## Context

The `web/` directory has `package.json`, `schema.graphql`, and `Makefile` but zero source code, config files, or build tooling. The API is fully functional with GraphQL, JWT auth, and item CRUD. The web app needs to be scaffolded following the same patterns as the kapow reference project (`/home/ketan/project/kapow/web/`), adapted for Flotsam's domain (personal knowledge management).

## Scope

Scaffold the complete web frontend so it builds, lints, and tests clean. Includes:
- Build tooling (Vite, TypeScript, ESLint, PostCSS, Vitest configs)
- App shell (main.tsx, App.tsx, theme, providers)
- Auth (Zustand store with JWT login/register, urql auth exchange)
- GraphQL integration (gql.tada, strict wrappers, urql client)
- Core pages: Home (dashboard), Login, Register, Search, Capture (bookmark + note)
- Layout: Header with nav + auth, responsive
- Tests for all components

## Key Differences from Kapow

- **Auth**: Flotsam uses custom JWT (login/register mutations), NOT Supabase
- **Domain**: Items (bookmarks, notes, voice notes) instead of college search
- **No MCP web client** (no webmcp.js)
- **No carousel** (@mantine/carousel not in deps)
- **Icons**: Need to add `lucide-react` or use Mantine icons

## Missing Dependencies

Add to package.json:
- `@urql/exchange-auth` — JWT token injection into GraphQL requests
- `@0no-co/graphqlsp` — gql.tada TypeScript plugin
- `@testing-library/react` — component testing
- `@testing-library/jest-dom` — DOM matchers
- `@testing-library/user-event` — user interaction simulation
- `@eslint/js` — ESLint base config
- `globals` — browser globals for ESLint
- `typescript-eslint` — TS ESLint integration
- `eslint-plugin-react-hooks` — React hooks linting
- `eslint-plugin-react-refresh` — HMR safety
- `eslint-config-prettier` — Prettier compat
- `lucide-react` — icons

## File Plan

### Config files (adapt from kapow)
1. `web/index.html` — entry point, title "Flotsam", no Google Fonts (use system fonts)
2. `web/vite.config.ts` — port 8080, proxy `/graphql`, `/api`, `/mcp` → 8081
3. `web/tsconfig.json` — references app + node configs
4. `web/tsconfig.app.json` — ES2020, react-jsx, gql.tada plugin, `@/` alias, scalars: `DateTime: string, JSON: Record<string, unknown>`
5. `web/tsconfig.node.json` — ES2022 for build configs
6. `web/vitest.config.ts` — happy-dom, 80% coverage thresholds
7. `web/postcss.config.js` — postcss-preset-mantine + simple-vars
8. `web/eslint.config.js` — TS ESLint + react hooks + no-raw-graphql-strings rule + prettier
9. `web/eslint-rules/no-raw-graphql-strings.js` — custom rule (copy from kapow, rename plugin)

### Source files
10. `src/vite-env.d.ts` — Vite client types reference
11. `src/main.tsx` — StrictMode + UrqlProvider + MantineProvider + Notifications + BrowserRouter
12. `src/App.tsx` — Routes: `/` Home, `/login` Login, `/register` Register, `/search` Search, `/capture` Capture
13. `src/theme.ts` — Flotsam brand colors (teal/ocean palette for "flotsam" theme), Inter font

### Lib
14. `src/lib/graphql.ts` — strict useQuery/useMutation wrappers (from kapow)
15. `src/lib/urqlClient.ts` — urql client with authExchange reading from authStore (no Supabase)

### Stores
16. `src/stores/authStore.ts` — Zustand: token, user, login(), register(), logout(), getToken(), initialize() (reads from localStorage)

### Layout
17. `src/components/layout/Header.tsx` — Sticky header, nav links (Home, Search, Capture), auth menu
18. `src/components/layout/Header.test.tsx`

### Pages
19. `src/pages/Home.tsx` — Dashboard: recent items list, quick capture buttons
20. `src/pages/Home.test.tsx`
21. `src/pages/Login.tsx` — Email/password form, calls login mutation
22. `src/pages/Login.test.tsx`
23. `src/pages/Register.tsx` — Email/password/displayName form, calls register mutation
24. `src/pages/Register.test.tsx`
25. `src/pages/Search.tsx` — Search input + results list with type/tag filters
26. `src/pages/Search.test.tsx`
27. `src/pages/Capture.tsx` — Tabs: Bookmark (URL + title + tags) and Note (title + content + tags)
28. `src/pages/Capture.test.tsx`

### Test infrastructure
29. `src/test/setup.ts` — jest-dom matchers + cleanup
30. `src/test/render.tsx` — renderWithMantine helper
31. `src/test/mocks.ts` — mock urql client helper for testing GraphQL

### Assets
32. `src/assets/styles/index.css` — minimal global resets

## Implementation Order

1. Install missing dependencies (`pnpm add ...`)
2. Config files (index.html, tsconfig, vite, vitest, postcss, eslint)
3. Core source (vite-env.d.ts, theme.ts, lib/graphql.ts, lib/urqlClient.ts)
4. Auth store
5. Test infrastructure
6. Layout (Header)
7. App shell (main.tsx, App.tsx)
8. Pages (Home, Login, Register, Search, Capture) + tests
9. Generate graphql-env.d.ts (if API is available) or create stub
10. Verify: `pnpm build && pnpm lint && pnpm test`

## Verification

```bash
cd web
pnpm install        # all deps resolve
pnpm build          # tsc -b && vite build — zero errors
pnpm lint           # eslint --max-warnings 0
pnpm test           # vitest — all tests pass
```
