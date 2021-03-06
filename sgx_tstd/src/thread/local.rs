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

//! Thread local storage

use sgx_trts::enclave::{SgxGlobalData, SgxThreadPolicy};
use core::cell::UnsafeCell;
use core::mem;
use core::hint;
use core::fmt;
use core::intrinsics;

pub struct LocalKey<T: 'static> {
    // This outer `LocalKey<T>` type is what's going to be stored in statics,
    // but actual data inside will sometimes be tagged with #[thread_local].
    // It's not valid for a true static to reference a #[thread_local] static,
    // so we get around that by exposing an accessor through a layer of function
    // indirection (this thunk).
    //
    // Note that the thunk is itself unsafe because the returned lifetime of the
    // slot where data lives, `'static`, is not actually valid. The lifetime
    // here is actually slightly shorter than the currently running thread!
    //
    // Although this is an extra layer of indirection, it should in theory be
    // trivially devirtualizable by LLVM because the value of `inner` never
    // changes and the constant should be readonly within a crate. This mainly
    // only runs into problems when TLS statics are exported across crates.
    inner: unsafe fn() -> Option<&'static T>,
}

impl<T: 'static> fmt::Debug for LocalKey<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad("LocalKey { .. }")
    }
}

/// Declare a new thread local storage key of type [`std::thread::LocalKey`].
///
/// # Syntax
///
/// The macro wraps any number of static declarations and makes them thread local.
/// Publicity and attributes for each static are allowed. Example:
///
/// ```
/// use core::cell::RefCell;
/// thread_local! {
///     pub static FOO: RefCell<u32> = RefCell::new(1);
///
///     #[allow(unused)]
///     static BAR: RefCell<f32> = RefCell::new(1.0);
/// }
/// # fn main() {}
/// ```
///
#[macro_export]
#[allow_internal_unstable(thread_local_internals)]
macro_rules! thread_local {
    // empty (base case for the recursion)
    () => {};

    // process multiple declarations
    ($(#[$attr:meta])* $vis:vis static $name:ident: $t:ty = $init:expr; $($rest:tt)*) => (
        $crate::__thread_local_inner!($(#[$attr])* $vis $name, $t, $init);
        $crate::thread_local!($($rest)*);
    );

    // handle a single declaration
    ($(#[$attr:meta])* $vis:vis static $name:ident: $t:ty = $init:expr) => (
        $crate::__thread_local_inner!($(#[$attr])* $vis $name, $t, $init);
    );
}

#[macro_export]
#[allow_internal_unstable(thread_local_internals, cfg_target_thread_local, thread_local)]
#[allow_internal_unsafe]
macro_rules! __thread_local_inner {
    (@key $(#[$attr:meta])* $vis:vis $name:ident, $t:ty, $init:expr) => {
        {
            #[inline]
            fn __init() -> $t { $init }

             unsafe fn __getit() -> $crate::option::Option<&'static $t> 
            {
                #[thread_local]
                static __KEY: $crate::thread::LocalKeyInner<$t> =
                    $crate::thread::LocalKeyInner::new();

                __KEY.get(__init)
            }

            unsafe {
                $crate::thread::LocalKey::new(__getit)
            }
        }
    };
    ($(#[$attr:meta])* $vis:vis $name:ident, $t:ty, $init:expr) => {
        $(#[$attr])* $vis const $name: $crate::thread::LocalKey<$t> =
            $crate::__thread_local_inner!(@key $(#[$attr])* $vis $name, $t, $init);
    }
}

/// An error returned by [`LocalKey::try_with`](struct.LocalKey.html#method.try_with).
pub struct AccessError {
    _private: (),
}

impl fmt::Debug for AccessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AccessError").finish()
    }
}

impl fmt::Display for AccessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt("already destroyed", f)
    }
}

impl<T: 'static> LocalKey<T> {
    pub const unsafe fn new(inner: unsafe fn() -> Option<&'static T>) -> LocalKey<T> {
        LocalKey {
            inner,
        }
    }

    /// Acquires a reference to the value in this TLS key.
    ///
    /// This will lazily initialize the value if this thread has not referenced
    /// this key yet.
    ///
    /// # Panics
    ///
    /// This function will `panic!()` if TLS data needs to be destructed,
    /// TCS policy must be Bound.
    pub fn with<F, R>(&'static self, f: F) -> R
                      where F: FnOnce(&T) -> R {
        self.try_with(f).expect("if TLS data needs to be destructed, TCS policy must be Bound.")
    }
    /// Acquires a reference to the value in this TLS key.
    ///
    /// This will lazily initialize the value if this thread has not referenced
    /// this key yet. If the key has been destroyed (which may happen if this is called
    /// in a destructor), this function will return an [`AccessError`](struct.AccessError.html).
    ///
    /// # Panics
    ///
    /// This function will still `panic!()` if the key is uninitialized and the
    /// key's initializer panics.
    pub fn try_with<F, R>(&'static self, f: F) -> Result<R, AccessError>
    where
        F: FnOnce(&T) -> R,
    {
        unsafe {
            let thread_local = (self.inner)().ok_or(AccessError {
                _private: (),
            })?;
            Ok(f(thread_local))
        }
    }
}

pub struct LazyKeyInner<T> {
    inner: UnsafeCell<Option<T>>,
}

impl<T> LazyKeyInner<T> {
    pub const fn new() -> LazyKeyInner<T> {
        LazyKeyInner {
            inner: UnsafeCell::new(None),
        }
    }

    pub unsafe fn get(&self) -> Option<&'static T> {
        if intrinsics::needs_drop::<T>() {
            match SgxGlobalData::new().thread_policy() {
                SgxThreadPolicy::Unbound => {
                    return None;
                },
                SgxThreadPolicy::Bound => (),
            }
        }
        (*self.inner.get()).as_ref()
    }

    pub unsafe fn initialize<F: FnOnce() -> T>(&self, init: F) -> &'static T {
        // Execute the initialization up front, *then* move it into our slot,
        // just in case initialization fails.
        let value = init();
        let ptr = self.inner.get();

        // note that this can in theory just be `*ptr = Some(value)`, but due to
        // the compiler will currently codegen that pattern with something like:
        //
        //      ptr::drop_in_place(ptr)
        //      ptr::write(ptr, Some(value))
        //
        // Due to this pattern it's possible for the destructor of the value in
        // `ptr` (e.g., if this is being recursively initialized) to re-access
        // TLS, in which case there will be a `&` and `&mut` pointer to the same
        // value (an aliasing violation). To avoid setting the "I'm running a
        // destructor" flag we just use `mem::replace` which should sequence the
        // operations a little differently and make this safe to call.
        mem::replace(&mut *ptr, Some(value));

        // After storing `Some` we want to get a reference to the contents of
        // what we just stored. While we could use `unwrap` here and it should
        // always work it empirically doesn't seem to always get optimized away,
        // which means that using something like `try_with` can pull in
        // panicking code and cause a large size bloat.
        match *ptr {
            Some(ref x) => x,
            None => hint::unreachable_unchecked(),
        }
    }

    #[allow(unused)]
    pub unsafe fn take(&mut self) -> Option<T> {
        (*self.inner.get()).take()
    }
}

pub struct LocalKeyInner<T> {
    inner: LazyKeyInner<T>,
}

unsafe impl<T> Sync for LocalKeyInner<T> { }

impl<T> fmt::Debug for LocalKeyInner<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad("LocalKeyInner { .. }")
    }
}

impl<T> LocalKeyInner<T> {
    pub const fn new() -> LocalKeyInner<T> {
        LocalKeyInner {
            inner: LazyKeyInner::new(),
        }
    }

    pub unsafe fn get(&self, init: fn() -> T) -> Option<&'static T> {
        let value = match self.inner.get() {
            Some(ref value) => value,
            None => self.inner.initialize(init),
        };
        Some(value)
    }
}
