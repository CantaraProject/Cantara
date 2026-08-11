# The `.cantara.zip` selection file

A *selection* is the running order of a service: which elements are shown, in
which order, and what each of them is shown with. `.cantara.zip` is the file
that holds one — everything of it, so that opening the file on another
computer shows what its author saw.

It is an ordinary ZIP archive. Anyone can open it with the tools they already
have, look at what is in it and take a song out of it; nothing about it needs
Cantara to be understood. That is deliberate: a running order for next Sunday
should not be something only one program can read.

## What is in the archive

```text
selection.json          the running order and everything about it
assets/                 the elements themselves, under their own names
  Amazing Grace.song
  Handout.pdf
  Logo.png
```

Two entries, and that is the whole layout. `selection.json` is the manifest and
sits at the root; every file the running order refers to sits in `assets/`
under the name it had. An element that is used twice in one service is stored
once and referred to twice.

## `selection.json`

```json
{
  "format": "cantara.selection",
  "version": 1,
  "created_by": "Cantara 0.3.0",
  "designs": [ … ],
  "slide_settings": [ … ],
  "items": [ … ]
}
```

| Field | Meaning |
|---|---|
| `format` | Always `cantara.selection`. What tells a JSON file what it is. |
| `version` | The version of this layout. Cantara refuses an archive whose version it does not know rather than reading half of it. |
| `created_by` | Which Cantara wrote the file. For a human reading it; nothing depends on it. |
| `designs` | The presentation designs the elements refer to, as a list. A design used by ten songs is stored once and referred to by its position. |
| `slide_settings` | The same for the slide divisions — how a song is broken into slides. |
| `items` | The running order, in order. |

### An item

```json
{
  "file": "assets/Amazing Grace.song",
  "name": "Amazing Grace",
  "md5": "9e107d9d372bb6826bd81d3542a419d6",
  "design": 0,
  "slide_settings": null,
  "stream_design": null,
  "stream_slide_settings": null,
  "inline_markdown": null,
  "timer": { "timer_seconds": 30, "after_last_slide": "GoToNextChapter" },
  "transition": "Fade"
}
```

| Field | Meaning |
|---|---|
| `file` | Where the element is inside the archive. Absent for an element that is not a file — see `inline_markdown`. |
| `name` | What the element is called in the running order. |
| `md5` | The fingerprint of the file. What lets the opening Cantara recognise a song it already has, even under a different name — see *Opening* below. |
| `design` | Which entry of `designs` this element is shown with, by position. Absent means "whatever the opening Cantara uses generally". |
| `slide_settings` | The same, into `slide_settings`. |
| `stream_design`, `stream_slide_settings` | What the network stream shows instead, where that differs from the projection. |
| `inline_markdown` | Markdown typed into the program rather than read from a file. Such an element has no `file`. |
| `timer` | The automatic advance, where the element has one. Absent otherwise. |
| `transition` | How the slides of this element arrive: `None`, `Fade`, `SlideFromRight`, `SlideFromLeft`, `ZoomIn` or `Morph`. |

Every field except `file`, `name` and `items` itself may be absent, and an
absent field means the default. An archive written by a later Cantara that adds
a field is therefore still readable by an earlier one, which is why `version`
goes up only when that stops being true.

`designs` and `slide_settings` are written in the same shape the program's own
settings use, so that a design survives a round trip exactly. They are part of
the file rather than of the settings on purpose: that is what makes the archive
self-contained.

## Opening one

Opening a selection must not quietly change anything of the user's, so Cantara
reads the file first, says what it found, and only then does anything.

For every element it asks where it should come from:

1. **The library already has it.** Recognised by the fingerprint first and by
   the file name second: the same song under another name is the same song, and
   a song of the same name that has been edited since is still the one meant.
   The copy in the library is used and nothing is written.
2. **The library does not have it, and the archive brought it.** The element can
   be added to a repository the user picks — the one they picked last is
   remembered — or written to a folder that belongs to this run of the program,
   which leaves their library untouched and still lets the service run.
3. **Neither.** Nothing can be shown for it, so it is left out of the running
   order and counted in the dialog rather than appearing as an empty slide in
   front of a congregation.

An element that is written never replaces a file that is already there. A file
of the same name holding the same thing is used as it is; one holding something
else gets the new element beside it under a name of its own.

The designs and slide divisions the file brought are offered separately. The
elements carry their own copies either way — an imported selection looks the
way its author made it whether or not that offer is taken — and accepting it
adds the designs to the user's own list, where they can be picked and edited.

## The two Cantara 2 formats

Cantara 3 also reads and writes what Cantara 2 wrote, so that a running order
can travel between the two programs. Both hold **songs and their order and
nothing else**: a picture, a PDF or a Markdown document in the running order is
left out of them, as are the slide division, the timer, the transition and
everything about the network stream.

### `.songtex`

A TeX-like text file: a header of `%` comments and then one block per song.

```text
% This file has been created automatically
% It can be opened with Cantara (https://cantara.app)
% Manually editing the content may damage the import

\beginfile{Amazing Grace.song}
Amazing grace, how sweet the sound
that saved a wretch like me.
\endfile
\beginfile{And Can It Be.song}
…
\endfile
```

A `\noselection` line marks a file that is a collection of songs rather than a
running order. Cantara 3 reads such a file all the same — a collection put in
order is a running order, and refusing it would help nobody.

### Cantara 2's selection JSON

```json
{
  "version": 1,
  "songs": [
    {
      "file_name": "Amazing Grace.song",
      "file_content": "<the file, base64-encoded>",
      "style_setting": "default",
      "background_image": null
    }
  ]
}
```

`style_setting` may also be an object describing a font, a colour, an alignment
and a padding. Cantara 3 **reads past it**: it describes the look of a program
whose slides are laid out differently, and a design guessed from it would be a
design nobody chose. What the file is actually for — the songs and their order
— comes across exactly. For the same reason Cantara 3 writes `"default"` there
rather than inventing a mapping in the other direction.

## Where this lives in the code

| | |
|---|---|
| The formats, reading and writing | `src/logic/selection_io.rs` |
| Choosing a file and deciding what to keep | `src/components/selection_components/import_ui.rs` |
| Saving the running order | `src/components/selection_components/export_ui.rs` |

Adding a format means adding a variant to `SelectionFormat` and following the
compiler: every match on it is exhaustive.
