-- DocFlow Follow Up exception RFIs (ADR 0059).
--
-- Run once in the Supabase SQL editor after docs/supabase_auth.sql. Idempotent.
--
-- RFIs that the RFI Log records wrongly and that cannot be fixed there. The
-- Follow Up sync leaves them alone. The list is a preference shared by approved
-- users; it never authorizes a file change or an Undo.

create table if not exists public.follow_up_exceptions (
  code       text primary key,
  created_by uuid references auth.users(id) default auth.uid(),
  created_at timestamptz not null default now(),
  constraint follow_up_exceptions_code_ok
    check (code ~ '^AZE-SUN-INT-QMS-RFI-[A-Z]{3}-[0-9]{4}$')
);

alter table public.follow_up_exceptions enable row level security;

drop policy if exists follow_up_exceptions_read on public.follow_up_exceptions;
create policy follow_up_exceptions_read on public.follow_up_exceptions
  for select to authenticated using ( (select public.is_allowed()) );

drop policy if exists follow_up_exceptions_add on public.follow_up_exceptions;
create policy follow_up_exceptions_add on public.follow_up_exceptions
  for insert to authenticated with check ( (select public.is_allowed()) );

drop policy if exists follow_up_exceptions_remove on public.follow_up_exceptions;
create policy follow_up_exceptions_remove on public.follow_up_exceptions
  for delete to authenticated using ( (select public.is_allowed()) );

revoke all on public.follow_up_exceptions from anon;
grant select, insert, delete on public.follow_up_exceptions to authenticated;
