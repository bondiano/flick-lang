create table users (
  id bigint generated always as identity primary key,
  name text not null,
  email text
);
create table accounts (
  id bigint generated always as identity primary key,
  user_id bigint not null references users (id),
  balance numeric not null default 0,
  constraint balance_not_negative check (balance >= 0)
);
create table drift (id bigint primary key, amount text not null);
create table roles (id bigint primary key, role text not null);
insert into roles values (1, 'admin'), (2, 'root');
insert into drift values (1, 'twelve');
