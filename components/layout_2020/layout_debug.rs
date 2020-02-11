/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Supports writing a trace file created during each layout scope
//! that can be viewed by an external tool to make layout debugging easier.

use crate::flow::FragmentTreeRoot;
use serde_json::{to_string, to_value, Value};
use std::cell::RefCell;
use std::fs::File;
use std::io::Write;
use std::sync::Arc;

thread_local!(static STATE_KEY: RefCell<Option<State>> = RefCell::new(None));

pub struct Scope;

#[macro_export]
macro_rules! layout_debug_scope(
    ($($arg:tt)*) => (
        if cfg!(debug_assertions) {
            layout_debug::Scope::new(format!($($arg)*))
        } else {
            layout_debug::Scope
        }
    )
);

#[derive(Serialize)]
struct ScopeData {
    name: String,
    pre: Value,
    post: Value,
    children: Vec<Box<ScopeData>>,
}

impl ScopeData {
    fn new(name: String, pre: Value) -> ScopeData {
        ScopeData {
            name: name,
            pre: pre,
            post: Value::Null,
            children: vec![],
        }
    }
}

struct State {
    fragment: Arc<FragmentTreeRoot>,
    scope_stack: Vec<Box<ScopeData>>,
}

/// A layout debugging scope. The entire state of the fragment tree
/// will be output at the beginning and end of this scope.
impl Scope {
    pub fn new(name: String) -> Scope {
        STATE_KEY.with(|ref r| {
            if let Some(ref mut state) = *r.borrow_mut() {
                let fragment_tree = to_value(&state.fragment).unwrap();
                let data = Box::new(ScopeData::new(name.clone(), fragment_tree));
                state.scope_stack.push(data);
            }
        });
        Scope
    }
}

#[cfg(debug_assertions)]
impl Drop for Scope {
    fn drop(&mut self) {
        STATE_KEY.with(|ref r| {
            if let Some(ref mut state) = *r.borrow_mut() {
                let mut current_scope = state.scope_stack.pop().unwrap();
                current_scope.post = to_value(&state.fragment).unwrap();
                let previous_scope = state.scope_stack.last_mut().unwrap();
                previous_scope.children.push(current_scope);
            }
        });
    }
}

/// Begin a layout debug trace. If this has not been called,
/// creating debug scopes has no effect.
pub fn begin_trace(root: Arc<FragmentTreeRoot>) {
    assert!(STATE_KEY.with(|ref r| r.borrow().is_none()));

    STATE_KEY.with(|ref r| {
        let root_trace = to_value(&root).unwrap();
        let state = State {
            scope_stack: vec![Box::new(ScopeData::new("root".to_owned(), root_trace))],
            fragment: root.clone(),
        };
        *r.borrow_mut() = Some(state);
    });
}

/// End the debug layout trace. This will write the layout
/// trace to disk in the current directory. The output
/// file can then be viewed with an external tool.
pub fn end_trace(generation: u32) {
    let mut thread_state = STATE_KEY.with(|ref r| r.borrow_mut().take().unwrap());
    assert_eq!(thread_state.scope_stack.len(), 1);
    let mut root_scope = thread_state.scope_stack.pop().unwrap();
    root_scope.post = to_value(&thread_state.fragment).unwrap();

    let result = to_string(&root_scope).unwrap();
    let mut file = File::create(format!("layout_trace-{}.json", generation)).unwrap();
    file.write_all(result.as_bytes()).unwrap();
}
