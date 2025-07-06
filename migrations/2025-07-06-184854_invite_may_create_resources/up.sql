-- Your SQL goes here

alter table account_invites add column may_create_resources boolean not null default false;
