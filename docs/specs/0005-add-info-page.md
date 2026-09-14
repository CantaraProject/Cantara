# Add additional 'Abou the Program' info page

This spec describes the addition of an additional page on the same level as the selection, settings or detail view, showing information about the program.
The info view should be accessible via the settings, at the very buttom there should be a new section 'About the program' (headline) and a button 'Show information about the program' which routes to the info page.

## Content

The Info page should display the name of the app (Cantara), a short description 'Presentation software for churches' and the current version (get it from cargo environment variables). Below, the copyright should be shown as '(C) 2015-2026 Jan Martin Reckel' while the later year (2026) should be automatically updated during **compile time**.
Below, language specific information from the file `docs/about/cantara-info-<lang_code>.md` should be displayed which gets included during compile time.
For converting markdown to html, use the already imported crates of the project. The language code should be determined automatically.

## Page structure

Like all the aother pages: Header (containing the headline 'About the program', body and futoor (with a "Back" button returning to settings)

---

# What the brief leaves open

Ten questions the brief does not answer, each with the answer this spec
proposes. They are ordered by how expensive the wrong answer is later: the
first four change what has to be written down, the rest change only code.

## 1. The licence, and who has to be offered the source

Cantara is under the **AGPL**, and the web build is served to people over a
network. That is the licence's whole point of difference from the GPL: a
network user has to be able to get the source. Today the only place that says
so is `COPYING`, which nobody serving the web demo hands out, and the README,
which is on GitHub rather than in the program.

An about page is the natural — and, for the web build, arguably the required —
place for that. It costs one line and a link.

**Proposed:** the page names the licence and links to the source. In the
`.md` file rather than hardcoded, so it is translatable, but *not* optional:
see question 3 about what happens when a language file is missing.

**Answer:** Do as proposed.

## 2. Which version number the user is shown

`Cargo.toml` says `version = "0.3.0"`. The README says "This repository
contains version 3.0". Those are two different claims about the same program,
and the about page is where a user goes to find out which one to quote in a bug
report.

**Proposed:** show `CARGO_PKG_VERSION` — it is the one thing that is
mechanically true of the binary in front of the user, and it is already what
`settings_io` and `selection_io` stamp into exported files. If the intended
public version really is 3.0, that is a `Cargo.toml` change and it should
happen there, not be papered over on this page.

**Also worth deciding:** whether the page shows more than the version — a
commit hash and a build date are what turn "it does not work" into a reproducible
report. They are cheap here (`build.rs` already runs) and impossible to add
after the fact to a binary already in the wild.

**Answer:** Do as proposed (use the cargo.toml version). A commit hash is not necessary.

## 3. Which language code, and what happens when there is no file for it

`sys_locale::get_locale()` returns a full tag — `de-DE`, `en-GB`, `pt-BR`. The
brief asks for `cantara-info-<lang_code>.md`. Those are not the same alphabet.
`locales/*.yml` keys by primary subtag (`en`, `de`), and the about texts should
not invent a second convention.

**Proposed:** the primary subtag, lowercased, so `de-DE` and `de-AT` both read
`cantara-info-de.md`; fall back to `en` when there is no file, which is the
same fallback `rust_i18n::i18n!(fallback = "en")` already uses everywhere else.

**And the case the brief does not mention:** what if `cantara-info-en.md` is
missing too? The compile-time embedding means this is decidable at build time,
and a page with an empty body is the kind of defect that ships. `build.rs`
should **fail the build** when the fallback file is absent. A missing
translation is a normal state; a missing fallback is a broken program.

**Answer:** Use the primary subtag. If `cantara-info-en.md` is missing, the build shall fail.

## 4. What belongs in the `.md` file and what belongs in the markup

The brief puts the name, the description, the version and the copyright in the
markup, and "language specific information" in the file. Nothing stops the
`.md` from repeating the description in its own words, and it will: whoever
writes the German file will want to open with a sentence about what Cantara is.
Then there are two descriptions, one translated and one not, and they drift.

**Proposed:** write it down in the file that the `.md` starts *below* the
identity block — it is the prose about the project, not a restatement of what
is already on screen. The description in the markup stays a translation key
(`about.description`), not a hardcoded English string, so it is translated the
same way as every other sentence in the program.

**Answer:** Do as proposed.

## 5. The copyright year is the *build* year, and that has two consequences

The brief asks for the later year to be updated at compile time. Two things
follow that are worth accepting deliberately rather than discovering:

* **It breaks byte-for-byte reproducible builds across a new year.** The same
  source produces a different binary on 31 December and 1 January. If
  reproducibility is ever wanted, the conventional fix is to honour
  `SOURCE_DATE_EPOCH` when it is set and use the wall clock otherwise. That is
  three lines and worth having now rather than retrofitting.
* **Cargo will not rebuild for it.** `build.rs` reruns when a file or a named
  environment variable changes, and "the year changed" is neither. A
  development build carried across New Year shows the old year until something
  else forces a rebuild. This is harmless — a release build is always cold —
  but somebody will notice it and file it as a bug, so it belongs in a comment
  at the place that computes it.

**Proposed:** compute it in `build.rs` from `SOURCE_DATE_EPOCH` when set and
the system clock otherwise; `chrono` is already a dependency of the crate and
can be added to `[build-dependencies]` rather than doing calendar arithmetic by
hand. Document the staleness where it happens.

**Answer:** Do as proposed.

## 6. Where "Back" goes when the user did not come from the settings

The brief says the footer button returns to the settings. That is right when
the user arrived through the settings, which is the only door the brief builds.
But the route is an address: in the web build `…/Cantara/about` can be linked,
bookmarked and shared, and the AGPL source offer is exactly the kind of link
people send each other. A "Back" that lands such a visitor in the settings puts
them somewhere they have never been.

**Proposed:** the button goes to the settings regardless, and that is fine —
but it should say *where* it goes ("Back to settings") rather than "Back", so
it is a statement rather than a promise about history. Using the browser's
actual history instead is the wrong trade: it would take a user who arrived
from the settings back out of the program entirely on the web.

**Answer:** It should always go back to where the user came from (the previews page).

## 7. The settings section has to be added in two places

`SettingsContent` keeps the list of sections for the jump sidebar and the
section elements themselves separately, and the comment there already warns
what happens when they disagree: "an entry without a section is an anchor
nothing can reach". A new "About the program" section means a new
`JumpTarget::section("settings-about", …)` **and** a new `section { id:
"settings-about", … }` at the end of the content.

**Proposed:** no change to that design — just do both, and let the section be
the last entry, which is what the brief asks for.

**Answer:** Do as proposed.

## 8. Raw HTML in the `.md` files reaches the page

`markdown_to_html` is the project's own wrapper and ends in `markdown::to_html`,
which passes raw HTML through, and the result goes into the page through
`dangerous_inner_html` the way the search excerpts already do. For *these*
files that is not a security question — they are compiled into the binary from
the repository, so anyone who can change them can change the program anyway.

**Proposed:** nothing to guard against, but the module doc should say it
outright, so that nobody later concludes the same treatment is safe for a file
a user supplies.

**Answer:** Do as proposed. nothing to guard against.

## 9. Third-party licences are not in scope, and should be said so

An about page in an open-source program is where people look for the
acknowledgements — the fonts in `assets/fonts/`, the crates, PicoCSS. Doing it
properly means generating a licence list at build time, which is a bigger job
than this spec.

**Proposed:** out of scope, stated as out of scope in this document so that the
next person does not have to work out whether it was forgotten. The link to the
source in question 1 is what carries the obligation in the meantime.

**Answer:** Do as proposed.

## 10. Does the page differ between desktop and web?

Everything on it — version, copyright, prose — is true of both. The stream
settings are `cfg`-ed out of the web build, so there is precedent for pages
that differ, and the question should be answered rather than assumed.

**Proposed:** identical in both. One page, no `cfg`.

**Answer:** Do as proposed. Identical content on all targets. PicoCSS ensures a responsive design.

---

# Implementation concept

Five pieces, in the order they can be built and checked. Nothing here depends
on a piece later in the list, so each step can be left in a working state.

## 1. The content, and getting it into the binary

**New files.** `docs/about/cantara-info-en.md` and `docs/about/cantara-info-de.md`
— prose about the project, beginning below the identity block (question 4), and
naming the licence and the source (question 1).

**`build.rs` gains one generator**, alongside `generate_bundled_fonts_data` and
`generate_bundled_repos_data`, following exactly their shape: write a `.rs` file
into `OUT_DIR`, `include!` it from a module. It emits

```rust
/// The about text for each language, by primary subtag, sorted.
pub const ABOUT_TEXTS: &[(&str, &str)] = &[("de", "…"), ("en", "…")];

/// The year this binary was built — see `0005` question 5.
pub const BUILD_YEAR: &str = "2026";
```

with `cargo:rerun-if-changed=docs/about` so an edited text is picked up, and a
hard error when no `en` entry was produced (question 3). The texts go in with
`include_str!` of an absolute path rather than being copied into the generated
file, so that the generated file stays readable when it has to be looked at.

**Why generated rather than a hand-written `match`:** adding a language becomes
adding a file, which is the same rule `locales/` already follows. A `match` arm
per language is a second place to forget.

## 2. `src/logic/about.rs` — the decisions, without a page

The whole of the thinking, testable without a browser, which is the tier this
project's spec [0004](0004-testing-playwright.md) says to reach for first:

```rust
/// The about text for a locale tag, falling back to English.
pub fn text_for(locale: &str) -> &'static str;

/// `"(C) 2015-2026 Jan Martin Reckel"`, with the end year from the build.
pub fn copyright() -> String;

/// What this binary calls itself.
pub fn version() -> &'static str;   // CARGO_PKG_VERSION
```

**Unit tests, which are the cheap ones and catch the whole of question 3:**
`de-DE` and `de-AT` both find the German text; an unknown language gets the
English one; a bare `de` works as well as a tagged one; the copyright ends in
the build year and begins in 2015.

`FIRST_YEAR` is a constant here with a comment saying it is the year Cantara
was first released, so that it is never "updated" alongside the other one.

## 3. `src/components/about_components.rs` — the page

The three-part shell every other page uses (`wrapper` / `top-bar` / content /
`bottom-bar`), copied from `SettingsPage` rather than invented:

```rsx
div { class: "wrapper",
    header { class: "top-bar", h2 { {t!("about.headline")} } }
    main { class: "container-fluid content height-100",
        hgroup {
            h1 { "Cantara" }
            p { {t!("about.description")} }
        }
        p { {t!("about.version", version => about::version())} }
        p { {about::copyright()} }
        div { dangerous_inner_html: "{markdown_to_html(about::text_for(&locale))}" }
    }
    footer { class: "bottom-bar",
        button { onclick: …, {t!("about.back_to_settings")} }
    }
}
```

The locale comes from `sys_locale::get_locale()`, the same call `App` makes
before `rust_i18n::set_locale` — not from `rust_i18n`, which has no reader for
the locale it was given.

**`Cantara` is a name and stays untranslated**; everything else is a key.

## 4. Wiring: route, settings entry, translations

**`src/main.rs`** gains a route beside the other pages:

```rust
/// What the program is, who wrote it, and under what licence.
#[route("/about")]
AboutPage {},
```

**`src/components/route_transitions.rs`** — its test asserts that two pages are
two elements. Adding `AboutPage` to it is one line and keeps the fade honest.

**`src/components/settings_components.rs`** — the section and its sidebar entry
(question 7), last in both lists, containing a heading and one button that
pushes `Route::AboutPage {}`.

**`locales/about.yml`** — a new file, merged automatically like the others,
carrying `_version: 2` (the comment in `common.yml` explains why that must be a
whole number) and the keys `about.headline`, `about.description`,
`about.version`, `about.back_to_settings`, in `en` and `de`.

## 5. What proves it works

Three tiers, and the point of naming all three is that the first two are nearly
free:

* **Unit tests** in `logic::about`, as above. They cover the language fallback,
  which is the part with actual logic in it.
* **A markup test** with `dioxus-ssr`, in the shape `slide_markup` uses: render
  `AboutPage` and assert the version, the copyright and some of the prose are
  *in the HTML*. This is the tier 0004 was written for — it catches the class
  where a page needs a browser event before it becomes itself, and a page whose
  body is empty because no text was found looks perfectly healthy otherwise.
* **One Playwright test**, now that the web build is driven by the suite: from
  the settings, the about section's button reaches the page, the page names the
  version, and the footer returns to the settings. `tests/browser/library.js`
  already opens the web build with a library; this needs a way in from the
  settings route and nothing else.

**What none of them check** is that the German text is good German, or that the
copyright year is the year a lawyer would want. Those need a person, and the
release checklist is where that belongs.

