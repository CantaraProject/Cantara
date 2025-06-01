# Cantara Rewrite in Dioxus (Rust) - Version 3
![GitHub branch check runs](https://img.shields.io/github/check-runs/CantaraProject/cantara/master)

This repository contains a rewrite of [Cantara](https://github.com/reckel-jm/cantara) in Rust with Dioxus which will be published as Cantara Version 3.0.

*Work is currently in progress.* Contributions are welcome!

Cantara is a song presentation software which allows to quickly create beautiful presentations with song lyrics, chords (yet to come), sheets (yet to come) and more. The current version is used in churches or small groups to present song lyrics and chords on a projector or a screen for a congregation or a group of people.

### Implementation Status of this Repository

As this is a rewrite, the implementation status of this repository is not directly comparable to the original Cantara repository. The following table shows the features that are currently implemented or planned for this version.

| Feature | Status |
| --- | --- |
| Song Lyrics Presentation | ✅ Implemented |
| Presentation Styling | Partially implemented |
| Chord Presentation | ❌ Not Implemented, in Progress |
| Image Presentation | ❌ Not Implemented, in Progress |
| PDF Presentation | ❌ Not Implemented, in Progress |
| Search Functionality | ❌ Not Implemented, in Progress |
| Import SongText Files | ❌ Not Implemented, in Progress |
| Export SongText Files | ❌ Not Implemented, in Progress |
| Export pptx Files | ❌ Not Implemented, in Progress |

If you would like additional features for Cantara, please feel free to open an issue or a pull request.

### Serving Your App

To develop the app locally, make sure to set up rust and dioxus as described in the [official documentation](https://dioxuslabs.com/learn/0.6/getting_started/#)

Run the following command in the root of your project to start developing with the default platform:

```bash
dx serve
```
