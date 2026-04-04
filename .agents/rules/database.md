# Database Conventions (PostgreSQL / pgx / Goose)

## Stack

- **PostgreSQL** with extensions as needed (PostGIS, pgvector, etc.)
- **Driver**: `jackc/pgx/v5` + `pgxpool` — context-aware, connection pooling
- **Migrations**: `pressly/goose/v3` embedded in binary via `//go:embed`

## Queries

Use pgx directly: `pool.QueryRow(ctx, sql, $1, $2, ...)`. Always pass context.
**Never build SQL with `fmt.Sprintf`** — always use `$N` parameterized placeholders.

## Creating a Migration

```bash
make migrate-create NAME=short_description
# creates migrations/NNNN_short_description.sql
```

Write the migration using Goose markers:

```sql
-- +goose Up
ALTER TABLE my_table ADD COLUMN foo text;

-- +goose Down
ALTER TABLE my_table DROP COLUMN foo;
```

Then apply:

```bash
make migrate-up
```

If the migration adds/removes/renames columns on a table that has fixture data, update the fixture's COPY headers and data to match.

## Testing

- **Always run database code against a real database.** Compilation and unit tests alone are not sufficient — runtime issues like pgx type encoding mismatches only surface with a live connection.
- Integration tests should skip gracefully when `DATABASE_URL` is unset.
- Never mock the database pool in integration tests.
