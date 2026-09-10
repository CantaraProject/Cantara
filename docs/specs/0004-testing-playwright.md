# Implement component and integration testing with playwright

Currently, Cantara has a lot of module tests. However, actual component tests and end-to-end integration tests are missing. Church services are very critical and the stability of a church service presentation should be handled with the same care as a nuclear reactor's safety system.
Therefore, we need component tests and extensive end-to-end integration tests which cover all use-cases:

- Presentation of several types (songs, PDFs, videos) with different settings (monitors, network streaming, network control, etc.)
- Test under high pressure (long pdf file), error cases (e.g. invalid pdf file, user specifies network url with invalid characters)
- Resistence against cyberattacks (Zip-Bombs in online-archives, DDos-Attack via network when streaming is activated).

This spec aims to design and implement these kind of testing scenarios while using all technologies and tools that are officailly descried in the documentation of Dioxus under https://dioxuslabs.com/learn/0.7/guides/testing/web.
- Component testing via `assert_rsx_eq`
- Hook testing via `test_hook`
- End to end testing via playwright
  - Create a testing environment with the https://github.com/reckel-jm/cantara-songrepo as default repo and public domain pdfs and videos (e.g. from wikipedia)
  - Create different settings which then can be tested
