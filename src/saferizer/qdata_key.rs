//! `QdataKey<T>` — a phantom-typed key that binds one GLib-qdata name to one
//! concrete stored type (ScrAP-10/12).
//!
//! GLib qdata is a per-`GObject` bag keyed by a plain string; the typed
//! accessors are `unsafe` because GLib stores the value as an untyped pointer
//! and reading it back at the *wrong* concrete type is instant undefined
//! behaviour, not merely a logic bug. This type makes that mismatch
//! **unrepresentable**: a `QdataKey<T>` const pairs a name with exactly one `T`
//! forever, and every read/write for that name goes through it — so the key and
//! its type can no longer drift apart by an eyeballed turbofish at a call site.
//!
//! **Contract for callers:** declare one `const KEY: QdataKey<T>` per qdata name
//! and route *all* access to that name through it. Never construct two keys with
//! the same name at different `T`, and never call the raw `set_data`/`data`/
//! `steal_data` for a name a `QdataKey` owns — those are the two acts this type
//! cannot prevent, and together they are the whole invariant. A `None` from
//! [`QdataKey::get`]/[`QdataKey::steal`] means the key was never set on that
//! object (or was already stolen), never a type mismatch.
//!
//! This module holds the codebase's sole `unsafe` for qdata.

use gtk::glib;
use gtk::prelude::*;
use std::marker::PhantomData;

/// A GLib-qdata name bound at the type level to the single concrete type `T`
/// stored under it. Construct one `const` per name (see module docs); its
/// methods are the only sanctioned way to touch that name's qdata.
pub(crate) struct QdataKey<T> {
    name: &'static str,
    _t: PhantomData<T>,
}

impl<T: 'static> QdataKey<T> {
    /// Bind qdata name `name` to the type `T`. One name ⇔ one `T`: declaring two
    /// keys with the same `name` at different `T` reintroduces the mismatch this
    /// type exists to forbid.
    pub(crate) const fn new(name: &'static str) -> Self {
        Self {
            name,
            _t: PhantomData,
        }
    }

    /// Store `val` on `obj` under this key, replacing any previous value.
    pub(crate) fn set(&self, obj: &impl IsA<glib::Object>, val: T) {
        // SAFETY: qdata's type-safety hazard is reading a name back at a
        // different type than was written. A `QdataKey<T>` binds this `name` to
        // exactly one `T` by construction, and it is the only accessor for the
        // name, so every `data::<T>`/`steal_data::<T>` uses the identical `T`
        // this `set_data::<T>` writes — the invariant is enforced by the type,
        // not by matching turbofish at call sites.
        unsafe {
            obj.set_data(self.name, val);
        }
    }

    /// Clone the value stored under this key, or `None` if the key was never set
    /// on `obj`. For an `Rc<_>` payload this is a cheap refcount bump.
    pub(crate) fn get(&self, obj: &impl IsA<glib::Object>) -> Option<T>
    where
        T: Clone,
    {
        // SAFETY: see `set` — the key↔`T` binding guarantees the stored value is
        // a `T`, so reading it at `T` and cloning is well-defined.
        unsafe { obj.data::<T>(self.name).map(|p| p.as_ref().clone()) }
    }

    /// Remove and return the value stored under this key, or `None` if it was
    /// never set (or was already stolen).
    pub(crate) fn steal(&self, obj: &impl IsA<glib::Object>) -> Option<T> {
        // SAFETY: see `set` — the key↔`T` binding guarantees the stored value is
        // a `T`, so taking ownership of it at `T` is well-defined.
        unsafe { obj.steal_data::<T>(self.name) }
    }
}
