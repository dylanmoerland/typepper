-- Players are GitHub accounts; a player's ssh keys are how the terminal game
-- knows them (docs/online.md, "Identity"). Web sessions come with web login.
create table players (
  id         bigint primary key,
  github_id  bigint not null unique,   -- the identity; logins can be renamed
  login      text   not null,          -- refreshed on every link
  created_at timestamptz not null default now()
);

create table ssh_keys (
  id          bigint primary key,
  fingerprint text   not null unique,  -- SHA256:... of the public key
  player_id   bigint not null references players on delete cascade,
  added_at    timestamptz not null default now()
);

alter table runs
  add constraint runs_player_id_fkey foreign key (player_id) references players on delete cascade;
