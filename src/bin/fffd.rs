use std::os::unix::ffi::OsStrExt;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};

use fff_search::file_picker::FilePicker;
use fff_search::grep::{GrepMode, GrepSearchOptions, has_regex_metacharacters};
use fff_search::{FFFMode, FilePickerOptions, GrepConfig, QueryParser, SharedFilePicker, SharedFrecency};

const SOCKET_PATH: &str = "/tmp/fffd.sock";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().collect::<Vec<_>>();

    let version             = args.iter().find(|f| f == &"--version").is_some();
    let do_cache            = args.iter().find(|f| f == &"--no-cache").is_none();
    let base_path           = args.iter().skip(1).find(|arg| !arg.starts_with('-')).cloned().unwrap_or_else(|| ".".into());

    if version {
        eprintln!("fff-grep 0.1.0 (fff-search 0.11.0)");
        return Ok(());
    }

    let shared_picker   = SharedFilePicker::default();
    let shared_frecency = SharedFrecency::default();

    FilePicker::new_with_shared_state(
        shared_picker.clone(),
        shared_frecency.clone(),
        FilePickerOptions {
            base_path,
            mode: FFFMode::Ai,
            enable_content_indexing: do_cache,
            watch: do_cache,
            enable_mmap_cache: do_cache,
            cache_budget: if do_cache {
                 Some(fff_search::types::ContentCacheBudget::unlimited())
            } else {
                None
            },
            ..Default::default()
        },
    )?;

    if do_cache && !shared_picker.wait_for_indexing_complete(std::time::Duration::from_secs(180)) {
        eprintln!("fffd: indexing still running after 180s, serving anyway");
    }

    _ = std::fs::remove_file(SOCKET_PATH);
    let listener = UnixListener::bind(SOCKET_PATH)?;

    eprintln!("fffd: {}listening on {SOCKET_PATH}", if do_cache { "indexed and " } else { "" });

    for conn in listener.incoming() {
        let shared_picker = shared_picker.clone();
        match conn {
            Ok(stream) => {
                std::thread::spawn(move || {
                    if let Err(e) = handle_client(stream, &shared_picker) {
                        eprintln!("fffd: client error: {e}");
                    }
                });
            }
            Err(e) => eprintln!("fffd: accept error: {e}"),
        }
    }

    Ok(())
}

fn handle_client(
    mut stream: UnixStream,
    shared_picker: &SharedFilePicker,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut line = String::new();
    BufReader::new(&stream).read_line(&mut line)?;

    let query_str    = line.trim();

    let mode         = if has_regex_metacharacters(query_str) {
        GrepMode::Regex
    } else {
        GrepMode::PlainText
    };

    let picker_guard = shared_picker.read()?;
    let picker       = picker_guard.as_ref().ok_or("index not ready yet")?;

    let query        = QueryParser::new(GrepConfig::default()).parse(query_str);
    let base_path    = picker.base_path();

    let results = picker.grep(
        &query,
        &GrepSearchOptions {
            mode,
            page_limit: 1_000_000_000,
            file_offset: 0,
            before_context: 0,
            after_context: 0,
            classify_definitions: false,
            trim_whitespace: false,
            max_matches_per_file: usize::MAX,
            max_file_size: u64::MAX,
            casing: Some(fff_search::grep::Casing::Sensitive),
            ..Default::default()
        },
    );

    if mode == GrepMode::Regex {
        if let Some(err) = &results.regex_fallback_error {
            eprintln!("fffd: invalid regex {query_str:?}, fell back to plain text: {err}");
        }
    }

    let mut out                = Vec::with_capacity(results.matches.len() * 150);  // @Tune

    let mut line_number_buffer = itoa::Buffer::new();

    let mut cached_file_index  = usize::MAX;
    let mut cached_path        = std::path::PathBuf::new();

    for m in &results.matches {
        let file = unsafe { results.files.get_unchecked(m.file_index) };

        if m.file_index != cached_file_index {
            cached_path       = file.absolute_path(picker, base_path);
            cached_file_index = m.file_index;
        }

        let path_bytes = cached_path.as_os_str().as_bytes();
        let num_str    = line_number_buffer.format(m.line_number);
        let line_bytes = m.line_content.as_bytes();

        let needed = path_bytes.len() + 1 + num_str.len() + 2 + line_bytes.len() + 1;
        out.reserve(needed);

        unsafe {
            push_unchecked(&mut out, path_bytes);
            push_unchecked(&mut out, b":");
            push_unchecked(&mut out, num_str.as_bytes());
            push_unchecked(&mut out, b": ");
            push_unchecked(&mut out, line_bytes);
            push_unchecked(&mut out, b"\n");
        }
    }

    stream.write_all(&out)?;
    Ok(())
}

#[inline(always)]
unsafe fn push_unchecked(out: &mut Vec<u8>, bytes: &[u8]) {
    let len = out.len();
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), out.as_mut_ptr().add(len), bytes.len());
        out.set_len(len + bytes.len());
    }
}
