# Handing a single setting on

Two of Cantara's settings are worth passing to somebody else on their own: the
**presentation design** a congregation has settled on, and the **slide
division** — the way a song is broken into slides. Both can be exported from
the settings page and are opened again with the same *Import* button in the
selection view as a running order: a user has a file, and which of Cantara's
kinds it is, is Cantara's problem rather than theirs.

| | Extension | Format |
|---|---|---|
| A design | `.cantara-design.zip` | ZIP archive |
| A slide division | `.cantara-slides.json` | JSON |

## A slide division: `.cantara-slides.json`

A division is a handful of switches and refers to nothing outside itself, so it
is a plain JSON file that can be read — and, carefully, edited — by hand.

```json
{
  "format": "cantara.slide_settings",
  "version": 1,
  "created_by": "Cantara 0.3.0",
  "slide_settings": {
    "name": "Two languages",
    "description": "For the international service",
    "title_slide": true,
    "show_spoiler": true,
    "show_meta_information": { "title_slide": false, "first_slide": true, "last_slide": true },
    "meta_syntax": "",
    "empty_last_slide": true,
    "max_lines": 4,
    "language": { "MultiLanguage": ["de", "en"] }
  }
}
```

`format` is what tells this apart from every other `.json` file in the world;
without it the file is offered to the readers for a Cantara 2 selection
instead. The division itself is written flat rather than nested one level
deeper, so the file reads as what it is.

The name and the description are Cantara 3's; the rest is the song library's
`SlideSettings` and is written by serde exactly as the settings hold it.

On import the division is added to the user's list — unless a division that
does the same thing is already there. What is compared is the switches, not the
name: the same division under somebody else's name is the same division.

## A design: `.cantara-design.zip`

A design refers to things outside itself — a background picture, a font that
not every computer has — and a design without them is not the design that was
handed over. So it is an archive:

```text
design.json      the design, and what its assets are called
assets/          the background picture and the font files
  Backdrop.jpg
  Open Sans.ttf
```

```json
{
  "format": "cantara.design",
  "version": 1,
  "created_by": "Cantara 0.3.0",
  "design": { "name": "Dark", "description": "…", "presentation_design_settings": { … } },
  "background_image": "assets/Backdrop.jpg",
  "fonts": [
    { "family": "Open Sans", "file": "assets/Open Sans.ttf" }
  ]
}
```

| Field | Meaning |
|---|---|
| `format` | Always `cantara.design`. Also what tells a design archive from a selection archive, since both are ZIP files. |
| `version` | The version of this layout. A file from a later Cantara is refused with a reason rather than read half-way. |
| `design` | The design itself, exactly as the settings hold it — serde in both directions, so a round trip loses nothing. |
| `background_image` | Where the picture is inside the archive. Absent when the design has none. |
| `fonts` | The font files that travel with the design, each with the family it stands for. |

### Which fonts travel

Only the ones that would not be there anyway:

- A family **Cantara bundles** is in every copy of the program.
- A **web-safe** family is on every computer.
- Everything else — a font installed on the machine the design was made on — is
  read from that machine and put into the archive.

A family whose file cannot be found is left out rather than refusing the
export: the design still opens, and the page falls back to something readable.
A font that only exists as a `.ttc` collection is skipped for the same reason —
a browser cannot load one.

### What importing does

1. The **design** is added to the user's list, unless exactly that design is
   already there.
2. The **background picture** is written into a repository the user picks — the
   same choice an imported song offers — so that it is also a picture they can
   use elsewhere. The design's copy of the path is then re-pointed at the file
   that was written: the path it arrived with is a path on somebody else's
   computer.
3. The **fonts** are kept in a folder of Cantara's own, beside the settings, and
   declared to the page as `@font-face` so that the family works by name exactly
   as a bundled one does. They are listed in the font picker under *Imported*.
   Nothing is installed into the operating system; the files belong to Cantara
   and go when its settings go.

A file that is already there is never overwritten — one holding the same thing
is used as it is, one holding something else gets the new file beside it under
a name of its own.

The web build has no file system: it keeps the design and says how much it had
to leave out.

## Where this lives in the code

| | |
|---|---|
| Both formats, reading and writing | `src/logic/settings_io.rs` |
| Keeping imported fonts usable | `src/logic/fonts.rs` |
| The export buttons | `src/components/settings_components.rs`, `src/components/song_slide_settings_components.rs` |
| Opening a file of any kind | `src/components/selection_components/import_ui.rs` |

The running order has a format of its own — see
[`cantara-zip.md`](cantara-zip.md).
