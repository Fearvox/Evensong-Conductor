create extension if not exists pgcrypto;

create table conductor_projects (
  id uuid primary key default gen_random_uuid(),
  slug text unique not null,
  name text not null,
  repo_url text not null,
  default_branch text not null default 'main',
  created_at timestamptz not null default now()
);

create table conductor_work_items (
  id uuid primary key default gen_random_uuid(),
  project_id uuid not null references conductor_projects(id) on delete cascade,
  source_kind text not null,
  source_id text not null,
  source_identifier text not null,
  source_url text,
  title text not null,
  state text not null,
  priority int,
  labels text[] not null default '{}',
  payload_redacted jsonb not null default '{}',
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  unique (source_kind, source_id)
);

create index conductor_work_items_project_state_priority_idx
  on conductor_work_items (project_id, state, priority, updated_at desc);

create index conductor_work_items_labels_idx
  on conductor_work_items using gin (labels);

create table conductor_runs (
  id uuid primary key default gen_random_uuid(),
  work_item_id uuid not null references conductor_work_items(id) on delete cascade,
  status text not null,
  policy_hash text not null,
  workspace_key text not null,
  started_at timestamptz not null default now(),
  ended_at timestamptz,
  failure_reason text
);

create index conductor_runs_work_item_started_idx
  on conductor_runs (work_item_id, started_at desc);

create index conductor_runs_status_started_idx
  on conductor_runs (status, started_at desc);

create table conductor_run_attempts (
  id uuid primary key default gen_random_uuid(),
  run_id uuid not null references conductor_runs(id) on delete cascade,
  attempt_number int not null,
  worker_kind text not null,
  worker_id text,
  model text,
  status text not null,
  started_at timestamptz not null default now(),
  ended_at timestamptz,
  error_summary text,
  unique (run_id, attempt_number)
);

create table conductor_workers (
  id uuid primary key default gen_random_uuid(),
  name text unique not null,
  kind text not null,
  host_label text not null,
  capabilities text[] not null default '{}',
  status text not null default 'unknown',
  last_heartbeat_at timestamptz
);

create index conductor_workers_status_heartbeat_idx
  on conductor_workers (status, last_heartbeat_at desc);

create index conductor_workers_capabilities_idx
  on conductor_workers using gin (capabilities);

create table conductor_worker_leases (
  id uuid primary key default gen_random_uuid(),
  worker_id uuid not null references conductor_workers(id) on delete cascade,
  run_attempt_id uuid references conductor_run_attempts(id) on delete set null,
  lease_key text not null,
  expires_at timestamptz not null,
  created_at timestamptz not null default now()
);

create index conductor_worker_leases_key_expiry_idx
  on conductor_worker_leases (lease_key, expires_at);

create index conductor_worker_leases_worker_expiry_idx
  on conductor_worker_leases (worker_id, expires_at);

create table conductor_events (
  id bigserial primary key,
  run_id uuid references conductor_runs(id) on delete cascade,
  run_attempt_id uuid references conductor_run_attempts(id) on delete cascade,
  event_type text not null,
  severity text not null default 'info',
  message text not null,
  payload_redacted jsonb not null default '{}',
  created_at timestamptz not null default now()
);

create index conductor_events_run_id_idx
  on conductor_events (run_id, id);

create index conductor_events_run_attempt_id_idx
  on conductor_events (run_attempt_id, id);

create index conductor_events_type_created_idx
  on conductor_events (event_type, created_at desc);

create table conductor_artifacts (
  id uuid primary key default gen_random_uuid(),
  run_id uuid references conductor_runs(id) on delete cascade,
  kind text not null,
  label text not null,
  uri text not null,
  sha256 text,
  redaction_level text not null default 'public-safe',
  created_at timestamptz not null default now()
);

create index conductor_artifacts_run_kind_created_idx
  on conductor_artifacts (run_id, kind, created_at desc);

create table conductor_model_usage (
  id uuid primary key default gen_random_uuid(),
  run_attempt_id uuid not null references conductor_run_attempts(id) on delete cascade,
  provider text not null,
  model text not null,
  input_tokens bigint not null default 0,
  output_tokens bigint not null default 0,
  total_tokens bigint not null default 0,
  context_window bigint,
  recorded_at timestamptz not null default now()
);

create index conductor_model_usage_attempt_recorded_idx
  on conductor_model_usage (run_attempt_id, recorded_at desc);

create index conductor_model_usage_provider_model_recorded_idx
  on conductor_model_usage (provider, model, recorded_at desc);

alter table conductor_projects enable row level security;
alter table conductor_work_items enable row level security;
alter table conductor_runs enable row level security;
alter table conductor_run_attempts enable row level security;
alter table conductor_workers enable row level security;
alter table conductor_worker_leases enable row level security;
alter table conductor_events enable row level security;
alter table conductor_artifacts enable row level security;
alter table conductor_model_usage enable row level security;

revoke all on table
  conductor_projects,
  conductor_work_items,
  conductor_runs,
  conductor_run_attempts,
  conductor_workers,
  conductor_worker_leases,
  conductor_events,
  conductor_artifacts,
  conductor_model_usage
from anon, authenticated;

revoke all on sequence conductor_events_id_seq from anon, authenticated;

grant all on table
  conductor_projects,
  conductor_work_items,
  conductor_runs,
  conductor_run_attempts,
  conductor_workers,
  conductor_worker_leases,
  conductor_events,
  conductor_artifacts,
  conductor_model_usage
to service_role;

grant usage, select on sequence conductor_events_id_seq to service_role;
