-- DocFlow — auth + allowlist + admin rolu.
-- Supabase SQL Editor'de BIR KEZ calistir:
--   https://supabase.com/dashboard/project/ndpkgbfmshceffwhpqoz/sql/new
--
-- Tasarim notlari:
--  * Admin bir DB BAYRAGIDIR (is_admin), e-posta STRINGI DEGIL. Aksi halde biri
--    admin'in adresiyle kayit olup admin olurdu. RLS auth.uid() ile dogrular.
--  * is_admin()/is_allowed() SECURITY DEFINER: app_users uzerindeki policy'nin
--    yine app_users'i okumasi RLS ozyinelemesi yaratirdi. Fonksiyonlar SADECE
--    cagiranin (auth.uid()) kendi durumunu doner — baskasinin verisini acmaz.
--  * history artik anon'a KAPALI; sadece "allowed" authenticated kullanicilar.

-- ---------------------------------------------------------------- app_users
create table if not exists public.app_users (
  id         uuid primary key references auth.users(id) on delete cascade,
  email      text not null,
  allowed    boolean not null default false,   -- admin onaylayana kadar KULLANAMAZ
  is_admin   boolean not null default false,
  created_at timestamptz not null default now()
);

-- Kayit olan herkes icin otomatik satir (allowed = false).
create or replace function public.handle_new_user()
returns trigger language plpgsql security definer set search_path = '' as $$
begin
  insert into public.app_users (id, email) values (new.id, new.email)
  on conflict (id) do nothing;
  return new;
end $$;

drop trigger if exists on_auth_user_created on auth.users;
create trigger on_auth_user_created
  after insert on auth.users
  for each row execute function public.handle_new_user();

-- ---------------------------------------------------------------- helpers
-- Sadece CAGIRANIN kendi durumunu doner (auth.uid()). RLS ozyinelemesini kirar.
create or replace function public.is_admin()
returns boolean language sql security definer stable set search_path = '' as $$
  select coalesce((select u.is_admin from public.app_users u
                   where u.id = (select auth.uid())), false)
$$;

create or replace function public.is_allowed()
returns boolean language sql security definer stable set search_path = '' as $$
  select coalesce((select u.allowed from public.app_users u
                   where u.id = (select auth.uid())), false)
$$;

-- ---------------------------------------------------------------- app_users RLS
alter table public.app_users enable row level security;
grant usage on schema public to authenticated;
grant select, update on public.app_users to authenticated;

drop policy if exists app_users_select on public.app_users;
create policy app_users_select on public.app_users for select to authenticated
  using ( (select auth.uid()) = id or public.is_admin() );

-- SADECE admin degistirebilir (kimi onayladigin buradan kontrol edilir).
drop policy if exists app_users_update on public.app_users;
create policy app_users_update on public.app_users for update to authenticated
  using ( public.is_admin() ) with check ( public.is_admin() );

-- ---------------------------------------------------------------- history kilitle
alter table public.history add column if not exists user_id uuid references auth.users(id);
alter table public.history add column if not exists user_email text;

-- anon erisimini KALDIR (artik giris zorunlu)
drop policy if exists history_anon_select on public.history;
drop policy if exists history_anon_insert on public.history;
drop policy if exists history_anon_update on public.history;
revoke select, insert, update on public.history from anon;
grant select, insert, update on public.history to authenticated;

drop policy if exists history_select_allowed on public.history;
create policy history_select_allowed on public.history for select to authenticated
  using ( public.is_allowed() );

drop policy if exists history_insert_allowed on public.history;
create policy history_insert_allowed on public.history for insert to authenticated
  with check ( public.is_allowed() and (user_id is null or user_id = (select auth.uid())) );

drop policy if exists history_update_allowed on public.history;
create policy history_update_allowed on public.history for update to authenticated
  using ( public.is_allowed() ) with check ( public.is_allowed() );

-- ---------------------------------------------------------------- BOOTSTRAP
-- 1) Once uygulamadan rohtolos@gmail.com ile KAYIT OL.
-- 2) Sonra bu satiri calistir (kendini onayla + admin yap):
--
--   update public.app_users set allowed = true, is_admin = true
--    where email = 'rohtolos@gmail.com';
--
-- 3) Artik admin panelinden digerlerini onaylayabilirsin.
