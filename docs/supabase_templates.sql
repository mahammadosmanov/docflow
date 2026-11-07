-- DocFlow shared document templates (ADR 0055).
--
-- Run once in the Supabase SQL editor after docs/supabase_auth.sql. The file is
-- idempotent: running it again repairs missing pieces without losing data.
--
-- Rules enforced here, not in the app:
--  * Approved users (is_allowed) read published templates and their files.
--  * Admins and granted editors (is_template_editor) create drafts, upload
--    files and publish. Nobody else can write.
--  * A published version is immutable: its file, checksum and field map never
--    change. Corrections are new versions. Storage objects are insert-only.

-- ---------------------------------------------------------------- editor role
alter table public.app_users
  add column if not exists can_edit_templates boolean not null default false;

create or replace function public.is_template_editor()
returns boolean language sql security definer stable set search_path = '' as $$
  select coalesce((select u.allowed and (u.is_admin or u.can_edit_templates)
                   from public.app_users u
                   where u.id = (select auth.uid())), false)
$$;

revoke all on function public.is_template_editor() from public;
grant execute on function public.is_template_editor() to authenticated;

-- ---------------------------------------------------------------- tables
create table if not exists public.doc_templates (
  id              uuid primary key default gen_random_uuid(),
  code            text not null,
  title           text not null,
  discipline      text not null default 'GEN',
  file_kind       text not null check (file_kind in ('xlsx', 'docx')),
  current_version integer,
  archived        boolean not null default false,
  created_by      uuid references auth.users(id) default auth.uid(),
  created_at      timestamptz not null default now(),
  updated_at      timestamptz not null default now(),
  constraint doc_templates_code_ok check (code ~ '^[A-Za-z0-9][A-Za-z0-9._-]{1,63}$'),
  constraint doc_templates_title_ok check (length(trim(title)) between 1 and 200),
  constraint doc_templates_discipline_ok check (discipline ~ '^[A-Z]{2,8}$')
);

create unique index if not exists doc_templates_code_key
  on public.doc_templates (upper(code));

create table if not exists public.doc_template_versions (
  id            uuid primary key default gen_random_uuid(),
  template_id   uuid not null references public.doc_templates(id) on delete restrict,
  version       integer not null check (version > 0),
  status        text not null default 'draft'
                check (status in ('draft', 'published', 'retired')),
  file_sha256   text not null check (file_sha256 ~ '^[0-9a-f]{64}$'),
  file_size     integer not null check (file_size between 1 and 20971520),
  storage_path  text not null,
  field_map     jsonb not null default '{"schema":1,"fields":[]}'::jsonb,
  note          text not null default '' check (length(note) <= 1000),
  created_by    uuid references auth.users(id) default auth.uid(),
  created_at    timestamptz not null default now(),
  published_at  timestamptz,
  unique (template_id, version)
);

-- Published versions are immutable; only the status may move to 'retired'.
create or replace function public.doc_template_versions_guard()
returns trigger language plpgsql set search_path = '' as $$
begin
  if old.status <> 'draft' then
    if new.file_sha256 is distinct from old.file_sha256
       or new.file_size is distinct from old.file_size
       or new.storage_path is distinct from old.storage_path
       or new.field_map is distinct from old.field_map
       or new.version is distinct from old.version
       or new.template_id is distinct from old.template_id
       or (old.status = 'retired' and new.status <> 'retired')
       or (old.status = 'published' and new.status not in ('published', 'retired')) then
      raise exception 'published_version_immutable' using errcode = '42501';
    end if;
  end if;
  return new;
end $$;

drop trigger if exists doc_template_versions_guard on public.doc_template_versions;
create trigger doc_template_versions_guard
  before update on public.doc_template_versions
  for each row execute function public.doc_template_versions_guard();

-- ---------------------------------------------------------------- publish
-- Publishing is one atomic step: the draft becomes published and the template
-- points at it. Only editors may call it.
create or replace function public.publish_template_version(p_version_id uuid)
returns integer language plpgsql security definer set search_path = '' as $$
declare v record;
begin
  if not public.is_template_editor() then
    raise exception 'not_template_editor' using errcode = '42501';
  end if;
  select id, template_id, version, status into v
    from public.doc_template_versions where id = p_version_id for update;
  if not found then
    raise exception 'version_not_found' using errcode = 'P0002';
  end if;
  if v.status <> 'draft' then
    raise exception 'version_not_draft' using errcode = '22023';
  end if;
  update public.doc_template_versions
     set status = 'published', published_at = now()
   where id = v.id;
  update public.doc_templates
     set current_version = v.version, updated_at = now(), archived = false
   where id = v.template_id;
  return v.version;
end $$;

revoke all on function public.publish_template_version(uuid) from public;
grant execute on function public.publish_template_version(uuid) to authenticated;

-- ---------------------------------------------------------------- manifest
-- One read tells a client everything it needs to decide what to download.
create or replace view public.doc_template_manifest
with (security_invoker = true) as
select t.id          as template_id,
       t.code,
       t.title,
       t.discipline,
       t.file_kind,
       v.id          as version_id,
       v.version,
       v.file_sha256,
       v.file_size,
       v.storage_path,
       v.field_map,
       v.published_at,
       t.updated_at
  from public.doc_templates t
  join public.doc_template_versions v
    on v.template_id = t.id and v.version = t.current_version
 where not t.archived and v.status = 'published';

-- ---------------------------------------------------------------- RLS
alter table public.doc_templates enable row level security;
alter table public.doc_template_versions enable row level security;

drop policy if exists doc_templates_select on public.doc_templates;
create policy doc_templates_select on public.doc_templates for select to authenticated
  using ( (select public.is_allowed()) );

drop policy if exists doc_templates_insert on public.doc_templates;
create policy doc_templates_insert on public.doc_templates for insert to authenticated
  with check ( (select public.is_template_editor()) );

drop policy if exists doc_templates_update on public.doc_templates;
create policy doc_templates_update on public.doc_templates for update to authenticated
  using ( (select public.is_template_editor()) )
  with check ( (select public.is_template_editor()) );

drop policy if exists doc_template_versions_select on public.doc_template_versions;
create policy doc_template_versions_select on public.doc_template_versions for select to authenticated
  using ( (select public.is_allowed())
          and (status <> 'draft' or (select public.is_template_editor())) );

drop policy if exists doc_template_versions_insert on public.doc_template_versions;
create policy doc_template_versions_insert on public.doc_template_versions for insert to authenticated
  with check ( (select public.is_template_editor()) and status = 'draft' );

drop policy if exists doc_template_versions_update on public.doc_template_versions;
create policy doc_template_versions_update on public.doc_template_versions for update to authenticated
  using ( (select public.is_template_editor()) )
  with check ( (select public.is_template_editor()) );

-- No delete policies: templates and versions are archived or retired, never deleted.

-- ---------------------------------------------------------------- storage
insert into storage.buckets (id, name, public, file_size_limit, allowed_mime_types)
values ('doc-templates', 'doc-templates', false, 20971520,
        array['application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',
              'application/vnd.openxmlformats-officedocument.wordprocessingml.document'])
on conflict (id) do update
  set public = false,
      file_size_limit = excluded.file_size_limit,
      allowed_mime_types = excluded.allowed_mime_types;

drop policy if exists doc_templates_objects_read on storage.objects;
create policy doc_templates_objects_read on storage.objects for select to authenticated
  using ( bucket_id = 'doc-templates' and (select public.is_allowed()) );

drop policy if exists doc_templates_objects_insert on storage.objects;
create policy doc_templates_objects_insert on storage.objects for insert to authenticated
  with check ( bucket_id = 'doc-templates' and (select public.is_template_editor()) );

-- No update or delete policies on template objects: uploads are immutable.

-- ---------------------------------------------------------------- admin notes
-- Grant template editing to a user (run as an admin in the SQL editor, or use
-- the DocFlow admin screen once it offers the toggle):
--   update public.app_users set can_edit_templates = true where email = '<email>';
