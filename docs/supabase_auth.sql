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

-- Kullanici adi. Bos (null) olabilir: eski kayitlar ve asagidaki yaris durumu.
alter table public.app_users add column if not exists username text;

-- Buyuk/kucuk harf FARK ETMEZ: "Ali" ile "ali" ayni kullanici adidir.
-- Ifade uzerinde unique index -> null'lar serbest (birden fazla null olabilir).
create unique index if not exists app_users_username_lower_key
  on public.app_users (lower(username));

-- Kayit olan herkes icin otomatik satir (allowed = false).
-- Kullanici adi signup'ta data:{username} olarak gelir -> raw_user_meta_data.
create or replace function public.handle_new_user()
returns trigger language plpgsql security definer set search_path = '' as $$
declare v_username text := nullif(trim(new.raw_user_meta_data->>'username'), '');
begin
  begin
    insert into public.app_users (id, email, username)
    values (new.id, new.email, v_username)
    on conflict (id) do nothing;
  exception when unique_violation then
    -- YARIS: iki kisi ayni adi ayni anda aldi. Kaydi PATLATMA — hesap acilsin,
    -- kullanici adi bos kalsin; kullanici Ayarlar'dan set_username ile duzeltir.
    -- (Uygulama zaten signup oncesi username_available ile kontrol ediyor.)
    insert into public.app_users (id, email) values (new.id, new.email)
    on conflict (id) do nothing;
  end;
  return new;
end $$;

drop trigger if exists on_auth_user_created on auth.users;
create trigger on_auth_user_created
  after insert on auth.users
  for each row execute function public.handle_new_user();

-- TELAFI: trigger SADECE yeni kayitlarda calisir. Bu dosyayi calistirmadan ONCE
-- kayit olmus biri varsa (ki oldu — sahibin hesabi bu tablodan once acildi),
-- auth.users'ta satiri VAR ama app_users'ta YOK. Sonucu sinsi: uygulama
-- allowed=false okur ve kullanici sonsuza kadar "Onay bekleniyor" ekraninda
-- kalir; "update app_users set allowed=true" ise SIFIR satir gunceller ve
-- sessizce basarili gorunur. Bu satir eksikleri doldurur ve dosyayi tekrar
-- calistirmayi guvenli/onarici yapar.
insert into public.app_users (id, email)
select u.id, u.email from auth.users u
on conflict (id) do nothing;

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

revoke all on function public.is_admin() from public;
revoke all on function public.is_allowed() from public;
grant execute on function public.is_admin() to authenticated;
grant execute on function public.is_allowed() to authenticated;

-- ---------------------------------------------------------------- kullanici adi
-- pgcrypto: Supabase'de genelde 'extensions' semasinda hazir gelir.
create extension if not exists pgcrypto with schema extensions;

-- Eski surumlerde anon'a acilan parola-denetleyen giris RPC'sini onarici
-- calistirmalarda da kaldir; kullanici adiyla giris guvenle desteklenmiyor.
drop function if exists public.email_for_login(text, text);

-- Kullanici adi kurallari TEK YERDE (sunucu). Istemci ayni kurali gosterir ama
-- zorlayan burasi — istemci dogrulamasi bir kolayliktir, guvenlik degil.
create or replace function public.username_ok(p text)
returns boolean language sql immutable set search_path = '' as $$
  select p is not null
     and length(p) between 3 and 24
     and p ~ '^[A-Za-z0-9._-]+$'
$$;

-- Kayit ekraninda "bu ad musait mi?" icin. anon cagirabilir.
-- Bilerek kabul edilen sizinti: bu, bir kullanici adinin VAR OLDUGUNU soyler.
-- Bundan kacinilamaz — "ad alinmis" uyarisi gosteren her uygulama bunu yapar.
-- E-POSTA sizdirmaz; sadece bool doner.
create or replace function public.username_available(p_username text)
returns boolean language sql security definer stable set search_path = '' as $$
  select public.username_ok(trim(p_username))
     and not exists (
       select 1 from public.app_users
        where lower(username) = lower(trim(p_username))
     )
$$;

-- Kendi kullanici adini degistir.
--
-- SECURITY DEFINER: app_users_update policy'si SADECE admin'e izin verir. Bu
-- fonksiyon o kapiyi bilerek asar ama YALNIZCA username sutununu ve YALNIZCA
-- auth.uid() satirini yazar — kullanici kendi allowed/is_admin bayragina
-- dokunamaz. Buraya baska sutun eklersen yetki yukseltme acmis olursun.
create or replace function public.set_username(p_username text)
returns text language plpgsql security definer set search_path = '' as $$
declare v text := nullif(trim(p_username), '');
begin
  if (select auth.uid()) is null then
    raise exception 'not_authenticated' using errcode = '28000';
  end if;
  if not public.username_ok(v) then
    raise exception 'username_invalid' using errcode = '22023';
  end if;
  if exists (select 1 from public.app_users
              where lower(username) = lower(v)
                and id <> (select auth.uid())) then
    raise exception 'username_taken' using errcode = '23505';
  end if;
  update public.app_users set username = v where id = (select auth.uid());
  return v;
end $$;

revoke all on function public.username_available(text) from public;
revoke all on function public.set_username(text) from public;
grant execute on function public.username_available(text) to anon, authenticated;
grant execute on function public.set_username(text) to authenticated;

-- Giris YAPMAMIS kullanici kayit sirasinda username_available cagirabilmeli.
-- Supabase varsayilani zaten boyle; asagida history icin anon'dan TABLO
-- yetkileri aliniyor — bu satir onunla karistirilmasin: sema kullanimi ayri sey.
grant usage on schema public to anon;

-- ---------------------------------------------------------------- app_users RLS
alter table public.app_users enable row level security;
grant usage on schema public to authenticated;
grant select, update on public.app_users to authenticated;

-- anon'un TABLO yetkisini al. RLS zaten satirlari gizliyor (policy'ler sadece
-- 'to authenticated'), yani bu tek basina bir acik kapatmiyor — KATMAN ekliyor:
-- ileride biri 'to' yan tumcesi olmayan bir policy yazarsa RLS tek savunma
-- olarak kalirdi ve kullanici listesi anon'a acilirdi. Iki kilit, tek anahtar degil.
-- Not: Supabase yeni public tablolara anon icin varsayilan SELECT verir; bu satir
-- olmadan app_users anon'a 200 [] doner (404/401 degil) — yani tablo VAR bilgisi
-- de sizar. username_available SECURITY DEFINER oldugu icin bundan etkilenmez.
revoke all on public.app_users from anon;

drop policy if exists app_users_select on public.app_users;
create policy app_users_select on public.app_users for select to authenticated
  using ( (select auth.uid()) = id or (select public.is_admin()) );

-- SADECE admin degistirebilir (kimi onayladigin buradan kontrol edilir).
drop policy if exists app_users_update on public.app_users;
create policy app_users_update on public.app_users for update to authenticated
  using ( (select public.is_admin()) )
  with check ( (select public.is_admin()) );

-- ---------------------------------------------------------------- history kilitle
-- Temiz bir Supabase projesinde de bu dosya tek basina calissin. Onceki surum
-- history tablosunun panelde daha once elle olusturuldugunu varsayiyordu.
create table if not exists public.history (
  id         uuid primary key default gen_random_uuid(),
  created_at timestamptz not null default now(),
  device     text not null default '',
  op_id      text not null,
  applied    integer not null default 0 check (applied >= 0),
  failed     integer not null default 0 check (failed >= 0),
  journal_id text not null,
  summary    text not null default '',
  steps      jsonb not null default '[]'::jsonb,
  undone     boolean not null default false,
  user_id    uuid references auth.users(id) on delete set null,
  user_email text
);

alter table public.history add column if not exists user_id uuid;
alter table public.history add column if not exists user_email text;

do $$
begin
  if not exists (
    select 1
      from pg_constraint
     where conname = 'history_user_id_fkey'
       and conrelid = 'public.history'::regclass
  ) then
    alter table public.history
      add constraint history_user_id_fkey
      foreign key (user_id) references auth.users(id) on delete set null;
  end if;
end $$;

-- Postgres foreign key sutunlarini otomatik indekslemez. user_id silme/join
-- islemleri ve uygulamanin created_at siralamasi bu indeksleri kullanir.
create index if not exists history_user_id_idx
  on public.history (user_id);
create index if not exists history_created_at_idx
  on public.history (created_at desc);

-- anon erisimini KALDIR (artik giris zorunlu)
drop policy if exists history_anon_select on public.history;
drop policy if exists history_anon_insert on public.history;
drop policy if exists history_anon_update on public.history;
revoke select, insert, update on public.history from anon;
revoke all on public.history from authenticated;
grant select, insert on public.history to authenticated;
grant update (undone) on public.history to authenticated;

drop policy if exists history_select_allowed on public.history;
create policy history_select_allowed on public.history for select to authenticated
  using ( (select public.is_allowed()) );

drop policy if exists history_insert_allowed on public.history;
create policy history_insert_allowed on public.history for insert to authenticated
  with check (
    (select public.is_allowed())
    and user_id = (select auth.uid())
  );

drop policy if exists history_update_allowed on public.history;
create policy history_update_allowed on public.history for update to authenticated
  using (
    (select public.is_allowed())
    and user_id = (select auth.uid())
  )
  with check (
    (select public.is_allowed())
    and user_id = (select auth.uid())
  );

-- ---------------------------------------------------------------- BOOTSTRAP
-- 1) Bu dosyanin TAMAMINI calistir. Tekrar calistirmak GUVENLI ve onaricidir
--    (yukaridaki TELAFI insert'i eksik app_users satirlarini doldurur).
-- 2) Supabase panelinde e-posta dogrulamasi KAPALI olmali:
--    Authentication -> Sign In / Providers -> Email -> "Confirm email" = OFF.
--    Gerekce: erisimi zaten admin onayi kontrol ediyor; dahili Supabase mailer
--    saatte birkac mesajla sinirli ve uretim icin degil — acik birakmak
--    ekibin kayitlarini ongorulemez sekilde bozar.
-- 3) Uygulamayi ac -> "Hesap oluştur" -> e-posta + kullanici adi + parola.
--    "Onay bekleniyor" ekranini goreceksin. DOGRUSU BU: herkes allowed = false
--    baslar, sahibi dahil. Sonraki girislerde e-posta adresini kullan;
--    guvenli, sunucu-tarafi hiz sinirli bir altyapi olmadan kullanici adiyla
--    giris bilerek desteklenmez.
-- 4) Kendini onayla + admin yap:
--
--   update public.app_users set allowed = true, is_admin = true
--    where email = 'rohtolos@gmail.com';
--
--    SIFIR satir guncellendi mi? O zaman app_users satirin yok (bkz. TELAFI) —
--    once bu dosyayi tekrar calistir, sonra bu update'i tekrar dene.
-- 5) Uygulamada "Tekrar denetle" -> iceri girersin, arac cubugunda kalkan
--    (admin) ikonu belirir.
-- 6) Bundan sonrasi panelden: yeni kisiler kendi kayit olur, sen Erisim'i acarsin.
--
-- Kontrol (kim var, kim onayli):
--   select email, username, allowed, is_admin, created_at
--     from public.app_users order by created_at;
--
-- auth.users ile app_users ayrismis mi (bos donmeli):
--   select u.email from auth.users u
--    where not exists (select 1 from public.app_users a where a.id = u.id);
