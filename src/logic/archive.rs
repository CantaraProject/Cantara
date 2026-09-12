//! Unpacking an archive somebody else made.
//!
//! # What this is defending against
//!
//! A song repository can be a ZIP file at a URL. Somebody adds one because a
//! link was passed on to them — in a group chat, in a newsletter, by whoever
//! set the church's computer up years ago — and Cantara downloads it and
//! unpacks it without anybody looking inside first. The archive is *entirely*
//! under the control of whoever serves that URL.
//!
//! Two things an archive can do that unpacking it naively will let it:
//!
//! **Write outside the folder it was unpacked into.** An entry may be named
//! anything, including `../../../.ssh/authorized_keys`. Joining that onto a
//! destination directory does not keep it inside — `Path::join` with an
//! absolute path throws the destination away entirely. This is old, has a
//! name — Zip Slip — and it is a remote write of arbitrary files.
//!
//! **Be much larger than it looks.** A few dozen kilobytes of zeroes compresses
//! to almost nothing, so a small download can unpack to more than the disk
//! holds. There is no clever detection to do: the fix is a budget, and refusing
//! to go past it.
//!
//! # Why the limits are checked twice
//!
//! Every entry declares its uncompressed size in the archive's own index. That
//! is worth checking — it is cheap and it turns away an obvious bomb before a
//! single byte is written — but it is **a number the attacker wrote**. An
//! archive can declare a kilobyte and deliver a gigabyte.
//!
//! So the budget is also enforced while reading, by giving the reader exactly
//! as much rope as is left. A lying header buys nothing: the read stops at the
//! limit whatever the index claimed.
//!
//! # Why one function and not two
//!
//! The desktop unpacks to a temporary folder and the web build into an
//! in-memory map. Those are genuinely different destinations, and they were
//! genuinely two copies of the same loop — so when this was written, neither
//! had any of the checks above and each would have had to grow them separately.
//! What they share is *deciding what is safe to take*, and that is what lives
//! here. Where the bytes go is the caller's business.

use std::io::Read;
use std::path::PathBuf;

use zip::ZipArchive;

/// How much an archive is allowed to be.
///
/// Generous on purpose. These are not a guess at what a repository needs;
/// they are the point past which something is no longer a song library, and
/// they are set so that a real one never meets them. A limit that legitimate
/// use runs into is a limit somebody raises without reading it.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// Everything the archive unpacks to, together.
    pub total_bytes: u64,
    /// A single file inside it.
    pub entry_bytes: u64,
    /// How many entries there may be.
    ///
    /// Separate from the byte budget because a million empty files costs
    /// almost no bytes and is still an attack: every one of them is an inode,
    /// a directory entry and a scan of the library afterwards.
    pub entries: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            // A gibibyte. The largest song repository anybody has is a few
            // dozen megabytes; a repository of scanned sheet music with a PDF
            // per song might reach a few hundred. A bomb is measured in
            // terabytes, so there is no overlap to get wrong.
            total_bytes: 1024 * 1024 * 1024,
            // A quarter of that for any one file — a video in a repository is
            // the largest legitimate thing, and one over this is not a song
            // library's problem.
            entry_bytes: 256 * 1024 * 1024,
            entries: 50_000,
        }
    }
}

/// Why an archive was not unpacked.
///
/// Carries the numbers, because the message reaches a person who added a
/// repository and needs to know whether they mistyped a URL or were handed
/// something hostile.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refused {
    /// More entries than [`Limits::entries`].
    TooManyEntries { found: usize, allowed: usize },
    /// The archive says it unpacks to more than [`Limits::total_bytes`], or it
    /// turned out to while being read.
    TooLarge { allowed: u64 },
    /// One file inside it is larger than [`Limits::entry_bytes`].
    EntryTooLarge {
        name: String,
        declared: u64,
        allowed: u64,
    },
    /// An entry is named in a way that would put it outside the folder it is
    /// being unpacked into.
    EscapesTheFolder { name: String },
    /// The archive itself could not be read.
    Unreadable { why: String },
}

impl std::fmt::Display for Refused {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Refused::TooManyEntries { found, allowed } => write!(
                formatter,
                "the archive holds {found} files, and at most {allowed} are unpacked"
            ),
            Refused::TooLarge { allowed } => write!(
                formatter,
                "the archive unpacks to more than {} MB, which is more than a song \
                 library should be",
                allowed / (1024 * 1024)
            ),
            Refused::EntryTooLarge {
                name,
                declared,
                allowed,
            } => write!(
                formatter,
                "{name} in the archive is {} MB, and at most {} MB is unpacked",
                declared / (1024 * 1024),
                allowed / (1024 * 1024)
            ),
            Refused::EscapesTheFolder { name } => write!(
                formatter,
                "the archive contains an entry named {name}, which would be written \
                 outside the folder it is unpacked into"
            ),
            Refused::Unreadable { why } => write!(formatter, "the archive could not be read: {why}"),
        }
    }
}

/// Hands every file in `archive` to `keep`, within `limits`.
///
/// `keep` is called with the entry's path — relative, and guaranteed to stay
/// inside whatever folder the caller joins it onto — and a reader for its
/// contents. That reader **stops at the budget that is left**, so a caller that
/// copies it to exhaustion cannot be made to write more than the limits allow
/// however the archive's index is written.
///
/// Directories are not passed on. A caller that needs them creates the parents
/// of the paths it is given, which is what both of Cantara's do anyway.
///
/// Stops at the first thing it refuses. Half an archive is not a repository,
/// and continuing past a hostile entry to see what else is in there is not a
/// service to anybody.
pub fn read_entries<R, F>(
    archive: &mut ZipArchive<R>,
    limits: Limits,
    mut keep: F,
) -> Result<(), Refused>
where
    R: std::io::Read + std::io::Seek,
    F: FnMut(&std::path::Path, &mut dyn Read) -> std::io::Result<()>,
{
    if archive.len() > limits.entries {
        return Err(Refused::TooManyEntries {
            found: archive.len(),
            allowed: limits.entries,
        });
    }

    // What is left of the budget. Counted down as the entries are read rather
    // than summed up beforehand, so that the declared sizes and the real ones
    // are held to the same total.
    let mut remaining = limits.total_bytes;

    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| Refused::Unreadable {
                why: error.to_string(),
            })?;

        if entry.is_dir() {
            continue;
        }

        // The safe reading of the name: `None` for anything absolute, anything
        // that climbs out with `..`, and anything with a NUL in it. The raw
        // name is only used to say which entry was refused.
        let Some(path): Option<PathBuf> = entry.enclosed_name() else {
            return Err(Refused::EscapesTheFolder {
                name: entry.name().to_string(),
            });
        };

        let declared = entry.size();
        if declared > limits.entry_bytes {
            return Err(Refused::EntryTooLarge {
                name: path.to_string_lossy().into_owned(),
                declared,
                allowed: limits.entry_bytes,
            });
        }
        if declared > remaining {
            return Err(Refused::TooLarge {
                allowed: limits.total_bytes,
            });
        }

        // One byte more than is left, so that reading it *dry* is
        // distinguishable from reading it to the end. Without the extra byte a
        // file of exactly the remaining size would look like one that had been
        // cut off.
        let ceiling = remaining.min(limits.entry_bytes).saturating_add(1);
        let mut bounded = entry.by_ref().take(ceiling);

        let mut counted = CountingReader {
            inner: &mut bounded,
            read: 0,
        };
        keep(&path, &mut counted).map_err(|error| Refused::Unreadable {
            why: error.to_string(),
        })?;
        let actually_read = counted.read;

        // The index said one thing and the file was another. Nothing legitimate
        // does this, and it is exactly what a bomb built to get past a
        // header check looks like.
        if actually_read >= ceiling {
            return Err(Refused::TooLarge {
                allowed: limits.total_bytes,
            });
        }

        remaining = remaining.saturating_sub(actually_read);
    }

    Ok(())
}

/// A reader that remembers how much came through it.
///
/// `io::copy` reports what it wrote, but a caller may do something else with
/// the reader entirely, and the budget has to be right either way. Counting at
/// the source is the only place that holds for every caller.
struct CountingReader<'a, R: Read> {
    inner: &'a mut R,
    read: u64,
}

impl<R: Read> Read for CountingReader<'_, R> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let count = self.inner.read(buffer)?;
        self.read = self.read.saturating_add(count as u64);
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Write};
    use zip::write::SimpleFileOptions;

    /// An archive built in memory, from (name, contents).
    ///
    /// Uses the `zip` writer, so the names are whatever that writer is willing
    /// to produce. For a name it is *not* willing to produce, see
    /// [`archive_with_a_raw_name`].
    fn archive(entries: &[(&str, &[u8])]) -> ZipArchive<Cursor<Vec<u8>>> {
        let mut buffer = Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut buffer);
            for (name, contents) in entries {
                writer
                    .start_file(*name, SimpleFileOptions::default())
                    .expect("an entry can be started");
                writer.write_all(contents).expect("and written");
            }
            writer.finish().expect("the archive is finished");
        }
        buffer.set_position(0);
        ZipArchive::new(buffer).expect("what was just written is readable")
    }

    /// An archive of one entry whose name is written **exactly** as given.
    ///
    /// # Why this exists
    ///
    /// The obvious way to test an absolutely named entry is to ask the `zip`
    /// writer for one. It will not give you one: it normalises the name on the
    /// way in and strips the leading slash, so the archive that comes out holds
    /// a harmless relative `etc/passwd` and the test passes for the wrong
    /// reason. That is what happened here first, and it is worth recording —
    /// **a hostile fixture built with a well-behaved tool is not hostile.**
    ///
    /// So the bytes are laid out by hand. A stored (uncompressed) entry, a
    /// central directory of one, and an end-of-central-directory record; the
    /// format is old and small enough that this is thirty lines.
    fn archive_with_a_raw_name(name: &str, contents: &[u8]) -> ZipArchive<Cursor<Vec<u8>>> {
        let name = name.as_bytes();
        let crc = crc32(contents);
        let size = contents.len() as u32;

        let mut bytes: Vec<u8> = Vec::new();

        // Local file header, then the data.
        bytes.extend(0x0403_4b50u32.to_le_bytes()); // signature
        bytes.extend(10u16.to_le_bytes()); // version needed
        bytes.extend(0u16.to_le_bytes()); // flags
        bytes.extend(0u16.to_le_bytes()); // stored, not deflated
        bytes.extend([0u8; 4]); // modification time and date
        bytes.extend(crc.to_le_bytes());
        bytes.extend(size.to_le_bytes()); // compressed
        bytes.extend(size.to_le_bytes()); // uncompressed
        bytes.extend((name.len() as u16).to_le_bytes());
        bytes.extend(0u16.to_le_bytes()); // no extra field
        bytes.extend(name);
        bytes.extend(contents);

        // The central directory, which is what a reader actually indexes from.
        let directory_at = bytes.len() as u32;
        bytes.extend(0x0201_4b50u32.to_le_bytes()); // signature
        bytes.extend(20u16.to_le_bytes()); // version made by
        bytes.extend(10u16.to_le_bytes()); // version needed
        bytes.extend(0u16.to_le_bytes()); // flags
        bytes.extend(0u16.to_le_bytes()); // stored
        bytes.extend([0u8; 4]); // modification time and date
        bytes.extend(crc.to_le_bytes());
        bytes.extend(size.to_le_bytes());
        bytes.extend(size.to_le_bytes());
        bytes.extend((name.len() as u16).to_le_bytes());
        bytes.extend([0u8; 6]); // extra, comment, disk number
        bytes.extend([0u8; 6]); // internal and external attributes
        bytes.extend(0u32.to_le_bytes()); // the local header is at the start
        bytes.extend(name);
        let directory_size = bytes.len() as u32 - directory_at;

        bytes.extend(0x0605_4b50u32.to_le_bytes()); // end of central directory
        bytes.extend([0u8; 4]); // disk numbers
        bytes.extend(1u16.to_le_bytes()); // entries on this disk
        bytes.extend(1u16.to_le_bytes()); // entries in total
        bytes.extend(directory_size.to_le_bytes());
        bytes.extend(directory_at.to_le_bytes());
        bytes.extend(0u16.to_le_bytes()); // no comment

        ZipArchive::new(Cursor::new(bytes)).expect("the hand-built archive is readable")
    }

    /// The checksum a ZIP entry carries.
    ///
    /// Written out rather than pulled in as a dependency: it is eight lines,
    /// and it is only here so that the hand-built archives above are
    /// well-formed enough for a reader to accept.
    fn crc32(bytes: &[u8]) -> u32 {
        let mut crc = 0xffff_ffffu32;
        for byte in bytes {
            crc ^= *byte as u32;
            for _ in 0..8 {
                let carry = crc & 1;
                crc >>= 1;
                if carry != 0 {
                    crc ^= 0xedb8_8320;
                }
            }
        }
        !crc
    }

    /// Everything that came out, as (path, contents).
    fn unpack(
        archive: &mut ZipArchive<Cursor<Vec<u8>>>,
        limits: Limits,
    ) -> Result<Vec<(String, Vec<u8>)>, Refused> {
        let mut taken = Vec::new();
        read_entries(archive, limits, |path, reader| {
            let mut contents = Vec::new();
            reader.read_to_end(&mut contents)?;
            taken.push((path.to_string_lossy().into_owned(), contents));
            Ok(())
        })?;
        Ok(taken)
    }

    /// The ordinary case, first: a repository unpacks.
    ///
    /// Worth stating before any of the refusals, because a check that refuses
    /// everything passes every one of the tests below.
    #[test]
    fn an_ordinary_archive_is_unpacked_whole() {
        let mut zip = archive(&[
            ("songs/Amazing Grace.song", b"Amazing grace"),
            ("songs/nested/Alas.song", b"Alas, and did"),
            ("README.md", b"# Lieder"),
        ]);

        let taken = unpack(&mut zip, Limits::default()).expect("an ordinary archive is fine");

        assert_eq!(taken.len(), 3);
        assert_eq!(taken[0].1, b"Amazing grace");
    }

    // ── Writing outside the folder ───────────────────────────────────────

    /// An entry that climbs out of the destination is refused.
    ///
    /// The whole archive, not just that entry: an archive containing one is
    /// not a song repository with a mistake in it.
    #[test]
    fn an_entry_that_climbs_out_of_the_folder_is_refused() {
        let mut zip = archive(&[
            ("songs/Amazing Grace.song", b"Amazing grace"),
            ("../../../.ssh/authorized_keys", b"ssh-rsa AAAA..."),
        ]);

        assert!(
            matches!(
                unpack(&mut zip, Limits::default()),
                Err(Refused::EscapesTheFolder { .. })
            ),
            "an archive was allowed to write outside the folder it was unpacked into"
        );
    }

    /// **The property all of this is for**: nothing an archive can be named
    /// puts a file outside the folder it is unpacked into.
    ///
    /// Stated as the thing that must not happen rather than as a list of
    /// checks, because the checks are not the point and there is more than one
    /// way to satisfy it. An entry may be *refused* — that is what a name
    /// climbing out with `..` gets — or it may be *brought back inside*, which
    /// is what happens to an absolute one: a leading `/` is simply dropped and
    /// `/etc/passwd` becomes `etc/passwd` under the destination. Both are safe.
    /// Only landing outside is not.
    ///
    /// Written this way because a test that demanded one particular answer
    /// would have to be rewritten whenever the safe reading of a name changed,
    /// and would say nothing about a name nobody thought of. This one covers
    /// every name in the table with one sentence, and the table can grow.
    #[test]
    fn no_entry_can_be_written_outside_the_folder_it_is_unpacked_into() {
        let destination = std::path::Path::new("/srv/cantara/repository");

        for name in [
            "/etc/passwd",
            "/home/gemeinde/.bashrc",
            "C:\\Windows\\system32\\x.dll",
            "../../../.ssh/authorized_keys",
            "songs/../../../../etc/shadow",
            "..",
            "./../outside",
            "songs/./../../outside",
        ] {
            let mut zip = archive_with_a_raw_name(name, b"whatever");

            let mut landed: Vec<PathBuf> = Vec::new();
            let outcome = read_entries(&mut zip, Limits::default(), |path, _| {
                landed.push(destination.join(path));
                Ok(())
            });

            match outcome {
                // Refused outright: nothing was written at all.
                Err(Refused::EscapesTheFolder { .. }) => {}
                // Accepted: then wherever it landed is under the destination.
                Ok(()) => {
                    for path in &landed {
                        assert!(
                            path.starts_with(destination),
                            "{name} was unpacked to {}, which is outside {}",
                            path.display(),
                            destination.display()
                        );
                        assert!(
                            !path
                                .components()
                                .any(|part| part == std::path::Component::ParentDir),
                            "{name} was unpacked to {}, which climbs out once it is resolved",
                            path.display()
                        );
                    }
                }
                other => panic!("{name} gave {other:?}, which is neither safe nor a refusal"),
            }
        }
    }

    /// An absolutely named entry is brought inside rather than turned away.
    ///
    /// The specific half of the property above, named because it is the
    /// behaviour somebody reading this would most likely get wrong in either
    /// direction: it is safe, so it need not be refused, and it must not be
    /// joined on as it stands — `Path::join` with an absolute path throws the
    /// destination away entirely.
    #[test]
    fn an_absolutely_named_entry_is_brought_back_inside_the_folder() {
        let mut zip = archive_with_a_raw_name("/etc/passwd", b"root:x:0:0");

        let taken = unpack(&mut zip, Limits::default()).expect("it is made safe, not refused");

        assert_eq!(taken.len(), 1);
        assert_eq!(
            taken[0].0, "etc/passwd",
            "the leading slash survived, so joining this onto a destination \
             would write to the real /etc/passwd"
        );
    }

    /// The hand-built fixture really does carry the name it was given.
    ///
    /// A test of the test, and not idle: the whole point of building the bytes
    /// by hand is that the ordinary writer will not produce this name, and if
    /// the hand-built one quietly did not either then the test above would go
    /// green while asserting nothing at all. That is the failure mode this
    /// module exists to catch, so it should not be sitting in its own fixtures.
    #[test]
    fn the_hand_built_archive_carries_the_hostile_name() {
        let mut zip = archive_with_a_raw_name("/etc/passwd", b"x");

        assert_eq!(
            zip.by_index(0).expect("one entry").name(),
            "/etc/passwd",
            "the fixture normalised the name, so nothing hostile is being tested"
        );
    }

    /// `..` that goes down and comes back up again is fine.
    ///
    /// The rule is about where the entry *lands*, not about which characters
    /// are in it. Refusing every name containing `..` would turn away archives
    /// that are merely written oddly, and a check people work around is worse
    /// than none.
    #[test]
    fn a_name_that_comes_back_to_where_it_started_is_kept() {
        let mut zip = archive(&[("songs/../songs/Amazing Grace.song", b"Amazing grace")]);

        let taken = unpack(&mut zip, Limits::default()).expect("this stays inside");

        assert_eq!(taken.len(), 1);
    }

    // ── Being larger than it looks ───────────────────────────────────────

    /// An archive that unpacks to more than the budget is refused.
    #[test]
    fn an_archive_larger_than_the_budget_is_refused() {
        let mut zip = archive(&[("big", &vec![0u8; 4096])]);

        let refusal = unpack(
            &mut zip,
            Limits {
                total_bytes: 1024,
                ..Limits::default()
            },
        );

        assert!(
            matches!(refusal, Err(Refused::TooLarge { .. })),
            "got {refusal:?}"
        );
    }

    /// One very large file inside an otherwise reasonable archive is refused.
    #[test]
    fn a_single_oversized_file_is_refused() {
        let mut zip = archive(&[
            ("songs/Amazing Grace.song", b"Amazing grace"),
            ("enormous", &vec![0u8; 8192]),
        ]);

        let refusal = unpack(
            &mut zip,
            Limits {
                entry_bytes: 1024,
                ..Limits::default()
            },
        );

        assert!(
            matches!(refusal, Err(Refused::EntryTooLarge { .. })),
            "got {refusal:?}"
        );
    }

    /// The budget is a budget: several files that are each fine but together
    /// are not.
    ///
    /// This is what a real bomb looks like from the outside — nothing about any
    /// one entry is remarkable.
    #[test]
    fn entries_that_are_each_small_enough_are_still_refused_together() {
        let mut zip = archive(&[
            ("one", &vec![0u8; 600]),
            ("two", &vec![0u8; 600]),
            ("three", &vec![0u8; 600]),
        ]);

        let refusal = unpack(
            &mut zip,
            Limits {
                total_bytes: 1500,
                entry_bytes: 1024,
                ..Limits::default()
            },
        );

        assert!(
            matches!(refusal, Err(Refused::TooLarge { .. })),
            "each entry fit, so the total was never checked: {refusal:?}"
        );
    }

    /// An archive of a great many tiny files is refused on the count.
    ///
    /// Costs almost nothing in bytes, so the byte budget never notices. What it
    /// costs is a directory entry each and a library scan afterwards that walks
    /// every one of them.
    #[test]
    fn an_archive_of_too_many_files_is_refused() {
        let names: Vec<String> = (0..50).map(|number| format!("song-{number}.song")).collect();
        let entries: Vec<(&str, &[u8])> = names
            .iter()
            .map(|name| (name.as_str(), b"x" as &[u8]))
            .collect();
        let mut zip = archive(&entries);

        let refusal = unpack(
            &mut zip,
            Limits {
                entries: 10,
                ..Limits::default()
            },
        );

        assert!(
            matches!(refusal, Err(Refused::TooManyEntries { .. })),
            "got {refusal:?}"
        );
    }

    /// The budget holds even when the archive's index lies about it.
    ///
    /// The point of reading through a bounded reader rather than trusting
    /// [`zip::read::ZipFile::size`]. Every declared size in an archive is a
    /// number the person who built it chose, and a bomb built to get past a
    /// header check declares a kilobyte and delivers everything it has.
    ///
    /// The lie is made here by handing the reader a budget smaller than the
    /// file — the same situation from the reader's side — because a `zip`
    /// writer will not produce an index that disagrees with its own data.
    #[test]
    fn a_file_that_is_larger_than_its_index_says_does_not_get_past_the_budget() {
        let mut zip = archive(&[("plausible", &vec![0u8; 4096])]);

        // Read with a budget of one kilobyte. Whatever the index said, only a
        // kilobyte may be written — and going past it is a refusal rather than
        // a truncated file quietly landing on disk.
        let mut written = 0u64;
        let refusal = read_entries(
            &mut zip,
            Limits {
                total_bytes: 1024,
                entry_bytes: 1024 * 1024,
                ..Limits::default()
            },
            |_, reader| {
                let mut sink = std::io::sink();
                written += std::io::copy(reader, &mut sink)?;
                Ok(())
            },
        );

        assert!(
            matches!(refusal, Err(Refused::TooLarge { .. })),
            "got {refusal:?}"
        );
        assert!(
            written <= 1025,
            "{written} bytes were handed to the caller against a budget of 1024"
        );
    }

    /// A file of exactly the budget is not mistaken for one that overran it.
    ///
    /// The off-by-one this design invites: the reader is given one byte more
    /// than is left precisely so that "read to the end" and "hit the ceiling"
    /// are different observations. Without that, the largest archive that
    /// should be allowed is the first one refused.
    #[test]
    fn an_archive_of_exactly_the_budget_is_allowed() {
        let mut zip = archive(&[("exact", &vec![0u8; 1024])]);

        let taken = unpack(
            &mut zip,
            Limits {
                total_bytes: 1024,
                entry_bytes: 1024,
                ..Limits::default()
            },
        );

        assert!(
            taken.is_ok(),
            "an archive of exactly the allowance was refused: {taken:?}"
        );
    }

    /// An empty archive is not an error. It is an empty repository.
    #[test]
    fn an_empty_archive_is_read_as_nothing() {
        let mut zip = archive(&[]);

        assert_eq!(unpack(&mut zip, Limits::default()), Ok(Vec::new()));
    }

    /// The refusals say enough for somebody to act on.
    ///
    /// These messages reach a person who has just added a repository, and the
    /// difference between "you mistyped the URL" and "this was hostile" is the
    /// whole content of the message.
    #[test]
    fn a_refusal_says_what_was_wrong_with_the_archive() {
        let escapes = Refused::EscapesTheFolder {
            name: "../../.ssh/authorized_keys".to_string(),
        };
        assert!(escapes.to_string().contains("../../.ssh/authorized_keys"));
        assert!(escapes.to_string().contains("outside"));

        let large = Refused::TooLarge {
            allowed: 1024 * 1024 * 1024,
        };
        assert!(large.to_string().contains("1024 MB"));
    }
}
