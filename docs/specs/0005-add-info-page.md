# Add additional 'Abou the Program' info page

This spec describes the addition of an additional page on the same level as the selection, settings or detail view, showing information about the program.
The info view should be accessible via the settings, at the very buttom there should be a new section 'About the program' (headline) and a button 'Show information about the program' which routes to the info page.

## Content

The Info page should display the name of the app (Cantara), a short description 'Presentation software for churches' and the current version (get it from cargo environment variables). Below, the copyright should be shown as '(C) 2015-2026 Jan Martin Reckel' while the later year (2026) should be automatically updated during **compile time**.
Below, language specific information from the file `docs/about/cantara-info-<lang_code>.md` should be displayed which gets included during compile time.
For converting markdown to html, use the already imported crates of the project. The language code should be determined automatically.

## Page structure

Like all the aother pages: Header (containing the headline 'About the program', body and futoor (with a "Back" button returning to settings)

