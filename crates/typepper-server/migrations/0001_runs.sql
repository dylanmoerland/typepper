-- Uploaded rounds: the replay bytes, and what the server made of them by
-- playing them again (docs/online.md, "Database"). Players and their foreign
-- key come with accounts; migrations only ever add.
create table runs (
  id         bigint primary key,                 -- random, shown as 8 base62 chars
  player_id  bigint,                             -- null: anonymous
  replay     bytea  not null check (octet_length(replay) <= 1048576),
  engine     int    not null,
  mode       text   not null,                    -- Mode::key: 'time:30', 'pursuit:7', ...
  score      double precision,                   -- the server's own; null in zen
  wpm        double precision not null,
  accuracy   double precision not null,
  claimed    double precision,                   -- the client's number, to spot drift
  created_at timestamptz not null default now()
);

create index runs_age on runs (created_at);
