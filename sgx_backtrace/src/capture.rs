// Licensed to the Apache Software Foundation (ASF) under one
// or more contributor license agreements.  See the NOTICE file
// distributed with this work for additional information
// regarding copyright ownership.  The ASF licenses this file
// to you under the Apache License, Version 2.0 (the
// "License"); you may not use this file except in compliance
// with the License.  You may obtain a copy of the License at
//
//   http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing,
// software distributed under the License is distributed on an
// "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied.  See the License for the
// specific language governing permissions and limitations
// under the License..

use crate::{resolve, resolve_frame, trace, Symbol, SymbolName};
use std::fmt;
use std::path::{Path, PathBuf};
use std::prelude::v1::*;
use sgx_libc::c_void;

/// Representation of an owned and self-contained backtrace.
///
/// This structure can be used to capture a backtrace at various points in a
/// program and later used to inspect what the backtrace was at that time.
///
/// `Backtrace` supports pretty-printing of backtraces through its `Debug`
/// implementation.
///
/// # Required features
///
/// This function requires the `std` feature of the `backtrace` crate to be
/// enabled, and the `std` feature is enabled by default.
#[derive(Clone)]
#[cfg_attr(feature = "serialize", derive(Serializable, DeSerializable))]
pub struct Backtrace {
    // Frames here are listed from top-to-bottom of the stack
    frames: Vec<BacktraceFrame>,
    // The index we believe is the actual start of the backtrace, omitting
    // frames like `Backtrace::new` and `backtrace::trace`.
    actual_start_index: usize,
}

fn _assert_send_sync() {
    fn _assert<T: Send + Sync>() {}
    _assert::<Backtrace>();
}

/// Captured version of a frame in a backtrace.
///
/// This type is returned as a list from `Backtrace::frames` and represents one
/// stack frame in a captured backtrace.
///
/// # Required features
///
/// This function requires the `std` feature of the `backtrace` crate to be
/// enabled, and the `std` feature is enabled by default.
#[derive(Clone)]
pub struct BacktraceFrame {
    frame: Frame,
    symbols: Option<Vec<BacktraceSymbol>>,
}

#[derive(Clone)]
enum Frame {
    Raw(crate::Frame),
    #[allow(dead_code)]
    Deserialized {
        ip: usize,
        symbol_address: usize,
    },
}

impl Frame {
    fn ip(&self) -> *mut c_void {
        match *self {
            Frame::Raw(ref f) => f.ip(),
            Frame::Deserialized { ip, .. } => ip as *mut c_void,
        }
    }

    fn symbol_address(&self) -> *mut c_void {
        match *self {
            Frame::Raw(ref f) => f.symbol_address(),
            Frame::Deserialized { symbol_address, .. } => symbol_address as *mut c_void,
        }
    }
}

/// Captured version of a symbol in a backtrace.
///
/// This type is returned as a list from `BacktraceFrame::symbols` and
/// represents the metadata for a symbol in a backtrace.
///
/// # Required features
///
/// This function requires the `std` feature of the `backtrace` crate to be
/// enabled, and the `std` feature is enabled by default.
#[derive(Clone)]
#[cfg_attr(feature = "serialize", derive(Serializable, DeSerializable))]
pub struct BacktraceSymbol {
    name: Option<Vec<u8>>,
    addr: Option<usize>,
    filename: Option<PathBuf>,
    lineno: Option<u32>,
}

impl Backtrace {
    /// Captures a backtrace at the callsite of this function, returning an
    /// owned representation.
    ///
    /// This function is useful for representing a backtrace as an object in
    /// Rust. This returned value can be sent across threads and printed
    /// elsewhere, and the purpose of this value is to be entirely self
    /// contained.
    ///
    /// Note that on some platforms acquiring a full backtrace and resolving it
    /// can be extremely expensive. If the cost is too much for your application
    /// it's recommended to instead use `Backtrace::new_unresolved()` which
    /// avoids the symbol resolution step (which typically takes the longest)
    /// and allows deferring that to a later date.
    ///
    /// # Examples
    ///
    /// ```
    /// use sgx_backtrace::Backtrace;
    ///
    /// let current_backtrace = Backtrace::new();
    /// ```
    ///
    /// # Required features
    ///
    /// This function requires the `std` feature of the `backtrace` crate to be
    /// enabled, and the `std` feature is enabled by default.
    #[inline(never)] // want to make sure there's a frame here to remove
    pub fn new() -> Backtrace {
        let _guard = lock_and_platform_init();
        let mut bt = Self::create(Self::new as usize);
        bt.resolve();
        bt
    }

    /// Similar to `new` except that this does not resolve any symbols, this
    /// simply captures the backtrace as a list of addresses.
    ///
    /// At a later time the `resolve` function can be called to resolve this
    /// backtrace's symbols into readable names. This function exists because
    /// the resolution process can sometimes take a significant amount of time
    /// whereas any one backtrace may only be rarely printed.
    ///
    /// # Examples
    ///
    /// ```
    /// use sgx_backtrace::Backtrace;
    ///
    /// let mut current_backtrace = Backtrace::new_unresolved();
    /// println!("{:?}", current_backtrace); // no symbol names
    /// current_backtrace.resolve();
    /// println!("{:?}", current_backtrace); // symbol names now present
    /// ```
    ///
    /// # Required features
    ///
    /// This function requires the `std` feature of the `backtrace` crate to be
    /// enabled, and the `std` feature is enabled by default.
    #[inline(never)] // want to make sure there's a frame here to remove
    pub fn new_unresolved() -> Backtrace {
        let _guard = lock_and_platform_init();
        Self::create(Self::new_unresolved as usize)
    }

    fn create(ip: usize) -> Backtrace {
        let mut frames = Vec::new();
        let mut actual_start_index = None;
        trace(|frame| {
            frames.push(BacktraceFrame {
                frame: Frame::Raw(frame.clone()),
                symbols: None,
            });

            if frame.symbol_address() as usize == ip && actual_start_index.is_none() {
                actual_start_index = Some(frames.len());
            }
            true
        });

        Backtrace {
            frames,
            actual_start_index: actual_start_index.unwrap_or(0),
        }
    }

    /// Returns the frames from when this backtrace was captured.
    ///
    /// The first entry of this slice is likely the function `Backtrace::new`,
    /// and the last frame is likely something about how this thread or the main
    /// function started.
    ///
    /// # Required features
    ///
    /// This function requires the `std` feature of the `backtrace` crate to be
    /// enabled, and the `std` feature is enabled by default.
    pub fn frames(&self) -> &[BacktraceFrame] {
        &self.frames[self.actual_start_index..]
    }

    /// If this backtrace was created from `new_unresolved` then this function
    /// will resolve all addresses in the backtrace to their symbolic names.
    ///
    /// If this backtrace has been previously resolved or was created through
    /// `new`, this function does nothing.
    ///
    /// # Required features
    ///
    /// This function requires the `std` feature of the `backtrace` crate to be
    /// enabled, and the `std` feature is enabled by default.
    pub fn resolve(&mut self) {
        let _guard = lock_and_platform_init();
        for frame in self.frames.iter_mut().filter(|f| f.symbols.is_none()) {
            let mut symbols = Vec::new();
            {
                let sym = |symbol: &Symbol| {
                    symbols.push(BacktraceSymbol {
                        name: symbol.name().map(|m| m.as_bytes().to_vec()),
                        addr: symbol.addr().map(|a| a as usize),
                        filename: symbol.filename().map(|m| m.to_owned()),
                        lineno: symbol.lineno(),
                    });
                };
                match frame.frame {
                    Frame::Raw(ref f) => resolve_frame(f, sym),
                    Frame::Deserialized { ip, .. } => {
                        resolve(ip as *mut c_void, sym);
                    }
                }
            }
            frame.symbols = Some(symbols);
        }
    }
}

impl From<Vec<BacktraceFrame>> for Backtrace {
    fn from(frames: Vec<BacktraceFrame>) -> Self {
        Backtrace {
            frames,
            actual_start_index: 0,
        }
    }
}

impl Into<Vec<BacktraceFrame>> for Backtrace {
    fn into(self) -> Vec<BacktraceFrame> {
        self.frames
    }
}

impl BacktraceFrame {
    /// Same as `Frame::ip`
    ///
    /// # Required features
    ///
    /// This function requires the `std` feature of the `backtrace` crate to be
    /// enabled, and the `std` feature is enabled by default.
    pub fn ip(&self) -> *mut c_void {
        self.frame.ip() as *mut c_void
    }

    /// Same as `Frame::symbol_address`
    ///
    /// # Required features
    ///
    /// This function requires the `std` feature of the `backtrace` crate to be
    /// enabled, and the `std` feature is enabled by default.
    pub fn symbol_address(&self) -> *mut c_void {
        self.frame.symbol_address() as *mut c_void
    }

    /// Returns the list of symbols that this frame corresponds to.
    ///
    /// Normally there is only one symbol per frame, but sometimes if a number
    /// of functions are inlined into one frame then multiple symbols will be
    /// returned. The first symbol listed is the "innermost function", whereas
    /// the last symbol is the outermost (last caller).
    ///
    /// Note that if this frame came from an unresolved backtrace then this will
    /// return an empty list.
    ///
    /// # Required features
    ///
    /// This function requires the `std` feature of the `backtrace` crate to be
    /// enabled, and the `std` feature is enabled by default.
    pub fn symbols(&self) -> &[BacktraceSymbol] {
        self.symbols.as_ref().map(|s| &s[..]).unwrap_or(&[])
    }
}

impl BacktraceSymbol {
    /// Same as `Symbol::name`
    ///
    /// # Required features
    ///
    /// This function requires the `std` feature of the `backtrace` crate to be
    /// enabled, and the `std` feature is enabled by default.
    pub fn name(&self) -> Option<SymbolName> {
        self.name.as_ref().map(|s| SymbolName::new(s))
    }

    /// Same as `Symbol::addr`
    ///
    /// # Required features
    ///
    /// This function requires the `std` feature of the `backtrace` crate to be
    /// enabled, and the `std` feature is enabled by default.
    pub fn addr(&self) -> Option<*mut c_void> {
        self.addr.map(|s| s as *mut c_void)
    }

    /// Same as `Symbol::filename`
    ///
    /// # Required features
    ///
    /// This function requires the `std` feature of the `backtrace` crate to be
    /// enabled, and the `std` feature is enabled by default.
    pub fn filename(&self) -> Option<&Path> {
        self.filename.as_ref().map(|p| &**p)
    }

    /// Same as `Symbol::lineno`
    ///
    /// # Required features
    ///
    /// This function requires the `std` feature of the `backtrace` crate to be
    /// enabled, and the `std` feature is enabled by default.
    pub fn lineno(&self) -> Option<u32> {
        self.lineno
    }
}

impl fmt::Debug for Backtrace {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "stack backtrace:")?;

        let iter = if fmt.alternate() {
            self.frames.iter()
        } else {
            self.frames[self.actual_start_index..].iter()
        };

        for (idx, frame) in iter.enumerate() {
            // To reduce TCB size in Sgx enclave, we do not want to implement symbol resolution functionality.
            // Rather, we can print the offset of the address here, which could be later mapped to
            // correct function.
            let ip: *mut c_void = frame.ip();

            write!(fmt, "\n{:4}: ", idx)?;

            let symbols = match frame.symbols {
                Some(ref s) => s,
                None => {
                    write!(fmt, "<unresolved> ({:?})", ip)?;
                    continue;
                }
            };
            if symbols.len() == 0 {
                write!(fmt, "<no info> ({:?})", ip)?;
                continue;
            }

            for (idx, symbol) in symbols.iter().enumerate() {
                if idx != 0 {
                    write!(fmt, "\n      ")?;
                }

                if let Some(name) = symbol.name() {
                    write!(fmt, "{}", name)?;
                } else {
                    write!(fmt, "<unknown>")?;
                }

                if idx == 0 {
                    write!(fmt, " ({:?})", ip)?;
                }

                if let (Some(file), Some(line)) = (symbol.filename(), symbol.lineno()) {
                    write!(fmt, "\n             at {}:{}", file.display(), line)?;
                }
            }
        }

        Ok(())
    }
}

impl Default for Backtrace {
    fn default() -> Backtrace {
        Backtrace::new()
    }
}

impl fmt::Debug for BacktraceFrame {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("BacktraceFrame")
            .field("ip", &self.ip())
            .field("symbol_address", &self.symbol_address())
            .finish()
    }
}

impl fmt::Debug for BacktraceSymbol {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("BacktraceSymbol")
            .field("name", &self.name())
            .field("addr", &self.addr())
            .field("filename", &self.filename())
            .field("lineno", &self.lineno())
            .finish()
    }
}

fn lock_and_platform_init() {}

#[cfg(feature = "serialize")]
mod sgx_serialize_impls {
    use super::*;
    use sgx_serialize::{Serializable, DeSerializable, Encoder, Decoder};

    #[derive(Serializable, DeSerializable)]
    struct SerializedFrame {
        ip: usize,
        symbol_address: usize,
        symbols: Option<Vec<BacktraceSymbol>>,
    }

    impl DeSerializable for BacktraceFrame {
        fn decode<D>(d: &mut D) -> Result<Self, D::Error>
        where
            D: Decoder,
        {
            let frame: SerializedFrame = SerializedFrame::decode(d)?;
            Ok(BacktraceFrame {
                frame: Frame::Deserialized {
                    ip: frame.ip,
                    symbol_address: frame.symbol_address,
                },
                symbols: frame.symbols,
            })
        }
    }

    impl Serializable for BacktraceFrame {
        fn encode<E>(&self, e: &mut E) -> Result<(), E::Error>
        where
            E: Encoder,
        {
            let BacktraceFrame { frame, symbols } = self;
            SerializedFrame {
                ip: frame.ip() as usize,
                symbol_address: frame.symbol_address() as usize,
                symbols: symbols.clone(),
            }
            .encode(e)
        }
    }
}
