.PHONY: first-launch ledger-health hermes-health hermes-smoke console test fmt supabase-reset

DATABASE_URL ?= postgres://postgres:postgres@127.0.0.1:54322/postgres
HERMES_CAPTURE_LINES ?= 40

first-launch:
	./scripts/first-launch.sh

ledger-health:
	DATABASE_URL="$(DATABASE_URL)" cargo run -p conductor-core -- ledger-health

hermes-health:
	cargo run -p conductor-core -- hermes-health --capture-lines "$(HERMES_CAPTURE_LINES)"

hermes-smoke:
	DATABASE_URL="$(DATABASE_URL)" cargo run -p conductor-core -- hermes-health --capture-lines "$(HERMES_CAPTURE_LINES)" --smoke --write-event

console:
	DATABASE_URL="$(DATABASE_URL)" cargo run -p conductor-core -- serve-console

test:
	cargo test

fmt:
	cargo fmt --check

supabase-reset:
	supabase db reset
