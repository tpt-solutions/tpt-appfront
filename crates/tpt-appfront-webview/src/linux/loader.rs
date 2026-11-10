//! Runtime loading of native libraries for the Linux backend (stage 2 of
//! `docs/linux-webview-backend.md`).
//!
//! Nothing here links against GTK at build time. A library is opened by soname
//! when the backend starts, and each function the backend uses is looked up by
//! name. A missing library or symbol becomes a [`LoadError`] that names the
//! package to install, instead of a link failure or a crash later on.

use std::fmt;

use libloading::Library;

/// Why a native library or one of its symbols could not be loaded.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum LoadError {
    /// None of the candidate file names could be opened.
    LibraryNotFound {
        /// The names that were tried, in order.
        candidates: Vec<&'static str>,
        /// The distribution package that provides the library.
        package_hint: &'static str,
    },
    /// The library opened, but it does not export the symbol.
    SymbolMissing {
        /// The logical library name (e.g. `gtk-3`).
        library: &'static str,
        /// The C symbol that was looked up.
        symbol: &'static str,
    },
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoadError::LibraryNotFound {
                candidates,
                package_hint,
            } => write!(
                f,
                "could not load {} (tried {}); install the `{package_hint}` package to use the \
                 Linux webview",
                candidates.first().copied().unwrap_or("a native library"),
                candidates.join(", "),
            ),
            LoadError::SymbolMissing { library, symbol } => write!(
                f,
                "{library} does not provide `{symbol}`; the installed version is too old or \
                 incompatible"
            ),
        }
    }
}

impl std::error::Error for LoadError {}

/// A dynamically loaded library.
///
/// Function pointers obtained through [`SharedLibrary::symbol`] are only valid
/// while this value is alive, so the owner must keep it for as long as the
/// pointers are used.
pub(crate) struct SharedLibrary {
    name: &'static str,
    lib: Library,
}

impl SharedLibrary {
    /// Opens the first of `candidates` that the system can load.
    ///
    /// `name` is the logical library name used in error messages, and
    /// `package_hint` names the package that provides it.
    pub(crate) fn open(
        name: &'static str,
        candidates: &[&'static str],
        package_hint: &'static str,
    ) -> Result<Self, LoadError> {
        for candidate in candidates {
            // SAFETY: opening a library runs its initializers. These are the system
            // GTK/GLib/WebKitGTK libraries the backend is meant to use.
            if let Ok(lib) = unsafe { Library::new(candidate) } {
                return Ok(SharedLibrary { name, lib });
            }
        }
        Err(LoadError::LibraryNotFound {
            candidates: candidates.to_vec(),
            package_hint,
        })
    }

    /// Looks up `symbol` and returns it as the function-pointer type `T`.
    ///
    /// # Safety
    ///
    /// `T` must be a function pointer type whose signature matches the C
    /// declaration of `symbol` in this library. A mismatch is undefined behavior
    /// when the pointer is called, and nothing here can check it.
    pub(crate) unsafe fn symbol<T: Copy>(&self, symbol: &'static str) -> Result<T, LoadError> {
        // SAFETY: the caller guarantees `T` matches the symbol's signature (see above).
        let found = unsafe { self.lib.get::<T>(symbol.as_bytes()) };
        match found {
            Ok(sym) => Ok(*sym),
            Err(_) => Err(LoadError::SymbolMissing {
                library: self.name,
                symbol,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A library every test host has, and a no-argument function it exports.
    #[cfg(target_os = "linux")]
    const PRESENT_LIB: &str = "libc.so.6";
    #[cfg(target_os = "linux")]
    const PRESENT_SYMBOL: &str = "getpid";
    #[cfg(target_os = "windows")]
    const PRESENT_LIB: &str = "kernel32.dll";
    #[cfg(target_os = "windows")]
    const PRESENT_SYMBOL: &str = "GetCurrentProcessId";

    #[test]
    fn missing_library_names_the_package() {
        let err = match SharedLibrary::open(
            "gtk-3",
            &["libtpt-no-such-library.so.9", "libtpt-no-such-library.so"],
            "libgtk-3-0",
        ) {
            Ok(_) => panic!("a library that does not exist must not open"),
            Err(err) => err,
        };

        assert_eq!(
            err,
            LoadError::LibraryNotFound {
                candidates: vec!["libtpt-no-such-library.so.9", "libtpt-no-such-library.so"],
                package_hint: "libgtk-3-0",
            }
        );
        let message = err.to_string();
        assert!(message.contains("libgtk-3-0"), "{message}");
        assert!(message.contains("libtpt-no-such-library.so.9"), "{message}");
    }

    #[test]
    fn falls_back_to_a_later_candidate() {
        let lib = SharedLibrary::open(
            "present",
            &["libtpt-no-such-library.so.9", PRESENT_LIB],
            "unused",
        );
        assert!(lib.is_ok(), "the second candidate should open");
    }

    #[test]
    fn resolves_an_existing_symbol() {
        let lib = SharedLibrary::open("present", &[PRESENT_LIB], "unused")
            .expect("the test host library should open");
        // SAFETY: the test symbol takes no arguments and returns an integer. Its
        // return type is only checked as 32-bit, which both declarations share.
        let f: unsafe extern "C" fn() -> u32 =
            unsafe { lib.symbol(PRESENT_SYMBOL) }.expect("the symbol should resolve");
        // SAFETY: see above; the library stays alive for the whole test.
        assert_ne!(unsafe { f() }, 0, "process id is never zero");
    }

    #[test]
    fn missing_symbol_names_the_library_and_symbol() {
        let lib = SharedLibrary::open("present", &[PRESENT_LIB], "unused")
            .expect("the test host library should open");
        // SAFETY: the symbol does not exist, so no pointer is returned to call.
        let err = match unsafe { lib.symbol::<unsafe extern "C" fn()>("tpt_no_such_symbol_xyz") } {
            Ok(_) => panic!("a missing symbol must not resolve"),
            Err(err) => err,
        };
        assert_eq!(
            err,
            LoadError::SymbolMissing {
                library: "present",
                symbol: "tpt_no_such_symbol_xyz",
            }
        );
        assert!(err.to_string().contains("tpt_no_such_symbol_xyz"));
    }
}
