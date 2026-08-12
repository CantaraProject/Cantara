# Tag mapping

A song file carries metadata as tags: `author`, `composer`, `copyright`, `ccli`
and whatever else the file's author chose to write down. The meta line of a
slide is a template that asks for them by name — `{{title}} ({{composer}})` —
and a tag the song does not have renders as nothing.

That is a problem as soon as a library is grown from more than one source.
One collection records the person who wrote the words under `author`, the next
under `writer`, a third under `poet`. A single template cannot fit all three,
and editing every file by hand to agree is a lot of work to undo later.

A **tag mapping** is a rule that says: where this song has `author`, read it as
`composer`.

Rules are configured under *Settings → Tag mapping*.

## What a rule does not do

**It never changes a song, and never changes a file.** The mapping is applied
to a copy on the way to the slides. The file on disk keeps the words its author
wrote, the detail view keeps showing them, and removing the rule puts
everything back exactly as it was.

This matters for a library shared between people: two users with different
mappings still have one text.

## The three rules

1. **A tag that is already there wins.** A mapping fills a gap; it never
   overwrites. A song carrying both `author` and `composer` is untouched by
   `author → composer`, because nothing was missing.

2. **One step, never a chain.** Every rule reads the song's *original* tags.
   With `author → composer` and `composer → arranger` configured, a song that
   only has `author` gets a `composer` and no `arranger`.

   This is why a cycle is harmless: `author → composer` together with
   `composer → author` simply means the two names are readable as each other,
   which is a reasonable thing to want.

3. **The first rule to fill a target wins.** Two rules pointing at the same
   target are not an error — `author → composer` and `writer → composer` is
   exactly how one unifies three collections. They are tried in the order they
   are listed.

Names are matched without regard to case or surrounding space, so a file
writing `Author:` is reached by a rule written as `author`.

## Where it applies

At slide generation: the presentation, its preview, the exports and the
network stream. Everything an audience sees goes through one function
(`slides_from_song_content`), and the mapping is applied there.

It deliberately does *not* apply in the detail view. The editor shows what is
in the file, because that is what it edits.

A classic `.song` file has no tags, so mappings do not reach it.

## A rule that is not finished

A rule with an empty side, or one pointing a name at itself, is marked in the
settings and passed over when the slides are built. A half-typed rule never
moves anything on screen.

## Where this lives in the code

- `src/logic/tag_mapping.rs` — the rule and the reading. Pure, and tested
  against each of the three rules above.
- `src/logic/settings.rs` — `Settings::tag_mappings`, the configured list.
- `src/logic/presentation.rs` — `slides_from_song_content`, where the mapping
  is applied.
- `src/components/settings_components.rs` — `TagMappingSection`.
