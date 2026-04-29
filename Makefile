.PHONY: first-launch ledger-health console test fmt supabase-reset

DATABASE_URL ?= postgres://postgres:postgres@127.0.0.1:54322/postgres

first-launch:
	./scripts/first-launch.sh

ledger-health:
	DATABASE_URL="$(DATABASE_URL)" cargo run -p conductor-core -- ledger-health

console:
	DATABASE_URL="$(DATABASE_URL)" cargo run -p conductor-core -- serve-console

test:
	cargo test

fmt:
	cargo fmt --check

supabase-reset:
	supabase db reset
