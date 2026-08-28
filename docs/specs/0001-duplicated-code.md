# 0001 — Duplicated code

Status: proposal. Written after fixing one bug that duplication had caused, in
order to find the rest of the same kind before they are found by a user.

The language of this document is English, as the rest of `docs/` is.

## The bug that started this

*Settings → presentation options → General* offered a list of slide settings
labelled "Folien-Einstellung 1", "Folien-Einstellung 2" — never the name the
user had given the entry. The *Specific* tab, the stream defaults, and the
per-element stream options, all offering the same list, showed the names
correctly.

There was no logic to the difference. The `<select>` was written six times —
three for presentation designs, three for slide settings — and five of them
called `SongSlideSettings::display_name`, while the sixth built its own label:

```rust
format!("{} {}", t!("selection.presentation_options.slide_settings"), index + 1)
```

That is exactly the fallback `display_name` uses for an *unnamed* entry, so the
panel looked plausible while silently ignoring every name in the settings.

**Fix.** [`presentation_options.rs`](../../src/components/selection_components/presentation_options.rs)
now has one `DesignSelect` and one `SlideSettingsSelect`. Each takes a label, an
optional `fallback` label (what an unmade choice means: "Default", "Same as the
presentation", or nothing where the choice cannot be left open), the chosen
position, and an `onselect` handler. All six call sites went through it; the
list of names now exists once and cannot disagree with itself.

Two things fell out of that change worth keeping in mind for the rest of this
document:

- The per-element panels stored the *chosen value* (a cloned `PresentationDesign`
  / `SlideSettings`), while the default panels stored a *position*. Reconciling
  them behind one component needed `design_position` / `slide_settings_position`,
  which look the value back up in the list. An entry edited since the choice was
  made no longer matches and the element reads as following the general setting
  — which is what it already did when presenting; the panel now says so.
- Removing the duplicate deleted two `use_memo`s in `DefaultDesignSettings` that
  existed only to feed the hand-written loops.

## What this document is for

Every finding below is duplication of the same shape: one idea written out more
than once, where the copies are *already* not identical, or where a rule lives
in several places and a change has to be made in all of them. They are ordered
by how likely they are to produce a bug of the kind above.

Nothing here is a request to make the code shorter. Shared code that hides a
real difference is worse than two honest copies; each item says what the shared
thing should be, not merely that one should exist.

---

## 1. Building a chapter from a selected element — `logic/presentation.rs`

**Where.** Three routines resolve an element's design and slide settings and
then build a `SlideChapter` from it:

- `create_presentation` (≈ line 487)
- `create_single_item_presentation` (≈ line 570)
- the position-preserving rebuild (≈ line 697)

All three open with the same eight lines:

```rust
let used_presentation_design = selected_item
    .presentation_design_option
    .clone()
    .unwrap_or(default_presentation_design.clone());

let used_slide_settings = selected_item
    .slide_settings_option
    .clone()
    .unwrap_or(default_slide_settings.clone());
```

and then fill in eleven `SlideChapter` fields, nine of them identically.

**Why it matters.** This is the rule that decides what the audience sees. It is
the same rule the panels in the bug above *describe*, so a change to how an
element falls back to the general setting has to be made in four places and
stay in agreement with a fifth. The three copies already differ in one field —
`create_single_item_presentation` deliberately leaves `stream_design_option`
empty, which is correct and is the only intended difference between them.

**Proposed shape.** A single `chapter_for(selected_item, defaults, tag_mappings,
stream: StreamView) -> Result<SlideChapter, _>`, where `StreamView` is an enum
of `Build(&StreamDefaults)` and `Skip` — the one real difference, named. The
rebuild's carried UUID stays with the rebuild: it is about identity across
rebuilds, not about the chapter's content.

**Effort.** Medium. Well covered by the existing tests in the same file.

---

## 2. The `<select>` over a named settings list (fixed here, still open elsewhere)

**Where.** Fixed for the six occurrences in `presentation_options.rs`. The
sibling pattern — a `<select>` over a list from the settings, addressed by
position, with a fallback entry — also appears in
`presentation_design_settings_components.rs` (`VerticalAlignmentSelector`,
`NotationAlignmentSelector`) and in the font settings, in each case over an enum
rather than a settings list.

**Why it matters.** Less than item 1: an enum's variants cannot be renamed by
the user, so the two cannot drift the way the slide settings did.

**Proposed shape.** Leave them. `DesignSelect` and `SlideSettingsSelect` cover
the case where the list belongs to the user; an enum selector is a different
thing that happens to render the same HTML.

---

## 3. The labelled slider

**Where.** At least nine copies, in
`font_settings.rs` (three), `presentation_design_settings_components.rs` (three),
`presenter_console_components.rs` (three). Each is:

```rust
label {
    { format!("{}: {}", t!("…"), value) }
    input {
        r#type: "range", min: "…", max: "…", step: "…",
        value: "{value}",
        oninput: { let base = subject.clone(); move |event: Event<FormData>| {
            let mut updated = base.clone();
            updated.field = event.value().parse::<f64>().unwrap_or(default);
            onchange.call(updated);
        } }
    }
}
```

The copies already differ in whether the label carries a unit (`" %"`), and in
what an unparseable value falls back to — `1.0`, `100.0`, or the field's own
default, chosen ad hoc per site.

**Why it matters.** The fallback-on-unparseable is a rule with three answers.
It rarely fires, which is precisely why nobody notices that one slider snaps to
something different from its neighbour.

**Proposed shape.** A `RangeInput` component taking label, unit, range, step,
current value, and `onchange: EventHandler<f64>` — value in, value out, with the
clone-and-mutate left at the call site where the field being written is visible.
Parsing failure resolves to the *current* value, which is the one answer that is
right everywhere: an unreadable event should change nothing.

**Effort.** Low, and each conversion is independently verifiable by eye.

---

## 4. Browser local storage — `cfg(target_arch = "wasm32")`

**Where.** Eighteen occurrences of

```rust
web_sys::window().and_then(|w| w.local_storage().ok().flatten())
```

in `presentation_components.rs` (ten), `presenter_console_components.rs` (four),
`selection_components.rs` (two), `logic/presentation.rs`, `logic/settings.rs`.
Around each sits a get-item / `serde_json::from_str` / ignore-the-error, or the
set-item counterpart, written out by hand every time.

**Why it matters.** Every one of these silently does nothing when storage is
unavailable, which is the correct behaviour and is re-derived eighteen times.
The key names (`SYNC_KEY_FILES`, `SYNC_KEY_PRESENTATION`, `"cantara-settings"`)
are strings shared between writer and reader across module boundaries, with
nothing but grep tying them together.

**Proposed shape.** A small `logic/web_storage.rs` with
`get<T: DeserializeOwned>(key) -> Option<T>`, `set<T: Serialize>(key, &T)`, and
`remove(key)`, plus the key constants in one place. Not a general abstraction —
just the three calls this program actually makes.

**Effort.** Low. Entirely `wasm32`-gated, so the desktop build is unaffected and
the web build either finds its data or does not, exactly as now.

---

## 5. The two settings editor pages

**Where.** `PresentationDesignSettingsPage` and `SongSlideSettingsPage` are the
same page:

- take an `index: u16` into a settings list,
- `use_memo` the entry out of the settings on every render (both carry a comment
  explaining why a local copy goes stale — one says it learned this "the hard
  way", the other cites the first),
- claim every hook before the early return,
- redirect to the settings page when the index has nothing behind it,
- `use_drop` with `try_read` to save on the way out,
- render a name/description `fieldset` — `MetaSettings` and
  `SlideSettingsMetadata`, which are the same component with different prop
  names and a placeholder on one of the two name fields.

**Why it matters.** This is the housekeeping around editing an entry in a named
list, and it is subtle: hook order, the early return, saving on drop. A third
such editor would be written by copying one of these two, inheriting whichever
of them happened to be copied.

**Proposed shape.** Two steps, and the second only if the first pays off:

1. One `MetadataFieldset { name, description, placeholder, on_changed }`
   replacing both metadata components. Immediate, small, no risk.
2. A `SettingsEntryEditor` wrapper taking the list accessor, the index, and the
   body as children, holding the memo/redirect/drop-save. Worth doing when a
   third editor appears; not before.

**Effort.** Step 1 low, step 2 medium.

---

## 6. The two named lists in `Settings`

**Where.** `presentation_designs: Vec<PresentationDesign>` and
`song_slide_settings: Vec<SongSlideSettings>` are maintained in lock-step:
`default_design_index` / `default_slide_settings_index`,
`default_presentation_design()` / `default_song_slide_settings()` (the same
get-or-first-or-default fallback twice), `ensure_default_presentation_design()`,
and `ensure_slide_settings_for_designs()` — which exists solely to pad the one
list up to the length of the other.

**Why it matters.** This is the root the bug grew from. Two lists that must stay
parallel, each with its own index, its own fallback and its own naming rule, is
what let one panel invent names for entries that had them. Note also that only
`SongSlideSettings` has `display_name`; `PresentationDesign` renders its `name`
field raw, so a design whose name the user clears shows as a blank option in
every one of these lists — the same gap in the other list, not yet reported.

**Proposed shape.** A `NamedList<T>` holding the entries and the default index,
with `default()`, `get_or_default(index)`, `display_name(index)` and
`ensure_non_empty()` written once, and `PresentationDesign` gaining the same
unnamed-entry fallback slide settings already have. `ensure_slide_settings_for_designs`
should then be re-examined rather than ported: padding one list to the other's
length is a workaround for the two being separate, and with a shared type it may
turn out to be unnecessary.

**Effort.** High — it touches the serialized settings format, so it needs a
migration path in `settings_io.rs` and the tests that pin the on-disk shape.
Worth scheduling deliberately, not doing in passing.

---

## 7. Test fixtures

**Where.** `SelectedItemRepresentation { … }` is written out field by field ten
times in `logic/presentation.rs`'s tests, twice in `selection_io.rs`; the latter
has an `item_of(name, path)` helper that the former does not use.
`logic/stream/protocol.rs` builds its own presentations again.

**Why it matters.** Least urgent of the seven, and the least likely to be seen
by a user. But every new field on `SelectedItemRepresentation` has to be added
to twelve literals, which is exactly the friction that discourages adding a
field where it belongs.

**Proposed shape.** Move `item_of` next to `SelectedItemRepresentation` under
`#[cfg(test)]` (or a `test_support` module) and have the presentation tests use
it, keeping the literals only where a test is *about* an unusual field
combination.

**Effort.** Low, mechanical, and failures are compile errors rather than
behaviour changes.

---

## Considered and rejected

- **`rfd::FileDialog` call sites** (six, across export, import, settings and the
  wizard). They differ in filters, in folder-versus-file, and in what they do
  with the result. What repeats is the crate's own API surface; there is no rule
  being duplicated.
- **`t!("…").to_string()`** everywhere. That is the i18n idiom, not duplication.
- **`settings.write() …; settings.read().save();`** as a pair. A single
  `update_and_save` would read well, but the two halves are separated by real
  logic often enough that the wrapper would be dodged where it mattered.

## Suggested order

1. Item 3 (sliders) and item 4 (local storage) — low effort, mechanical, each
   removes a rule with several answers.
2. Item 1 (chapter building) — highest value, since it decides what is shown.
3. Item 5 step 1, item 7 — small, opportunistic.
4. Item 6 — plan it; it needs a settings migration.

Item 2 needs nothing further.
