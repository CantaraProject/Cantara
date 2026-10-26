-- When a presentation last changed, so that one nobody ended — the laptop
-- that presented it was simply closed — can be told apart from one that is
-- still running, and ended and removed in its turn.
ALTER TABLE sessions ADD COLUMN last_active timestamptz NOT NULL DEFAULT now();
CREATE INDEX sessions_ended ON sessions (ended_at) WHERE ended_at IS NOT NULL;
