//! What an agent (an MCP client or a script on the control channel) changed last, so the window
//! can show it at work: a labelled cursor glides to the layers it just edited and they are
//! outlined for a moment. The engine only records which layers; the interface draws.

use std::collections::BTreeSet;
use std::sync::Arc;

use figcraft_doc::{Document, NodeId};
use serde_json::Value;

use crate::{Session, changed_nodes, params};

/// The name the agent's cursor carries until the preferences give another.
pub const DEFAULT_NAME: &str = "Claude";
/// Its colour until the preferences give another.
pub const DEFAULT_COLOR: &str = "#D9773B";
/// Most layers one edit points at (the rest still changed, they just aren't outlined).
pub const MAX_NODES: usize = 200;

/// The layers an agent's last command changed.
#[derive(Clone, Debug, PartialEq)]
pub struct AgentEdit {
    /// The document (`DocState::uid`) and page they are on.
    pub doc: u64,
    pub page: NodeId,
    pub nodes: Vec<NodeId>,
    /// Counts up with every edit, so the interface sees a new one even on the same layers.
    pub serial: u64,
}

/// The layers on `page` a command changed, from the document before and after it: the layers it
/// named (`ids`, `id`) that changed, else the selection it left when that changed (new layers are
/// selected), else the changed layers that contain no other changed layer (what was edited, not
/// the frames that re-flowed around it). Empty when nothing on the page changed.
pub fn edited(before: &Document, after: &Document, page: NodeId, params: &Value, selection: &[NodeId]) -> Vec<NodeId> {
    let changed: BTreeSet<NodeId> = changed_nodes(after, before).into_iter().filter(|n| *n != page && after.page_of(*n) == Some(page)).collect();
    if changed.is_empty() {
        return Vec::new();
    }
    // What contains a changed layer; a layer counts when it, or something inside it, changed.
    let around: BTreeSet<NodeId> = changed.iter().flat_map(|c| after.ancestors(*c)).collect();
    let touched = |n: &NodeId| changed.contains(n) || around.contains(n);
    let named: Vec<NodeId> = match params::get(params, "ids") {
        Some(Value::Array(a)) => a.iter().filter_map(params::id_of).collect(),
        _ => params::id(params, "id").into_iter().collect(),
    };
    let pick =
        |list: &[NodeId]| -> Vec<NodeId> { list.iter().copied().filter(|n| after.page_of(*n) == Some(page) && touched(n)).take(MAX_NODES).collect() };
    for list in [named.as_slice(), selection] {
        let nodes = pick(list);
        if !nodes.is_empty() {
            return nodes;
        }
    }
    changed.iter().copied().filter(|n| !around.contains(n)).take(MAX_NODES).collect()
}

impl Session {
    /// Record what the command an agent just ran changed in the document it ran on (`before`:
    /// that document's uid and contents before the command). Nothing changed on the page in
    /// front: the last edit stays.
    pub fn note_agent_edit(&mut self, before: Option<(u64, Arc<Document>)>, params: &Value) {
        let Some((uid, before)) = before else { return };
        let Some(ds) = self.docs.iter().find(|d| d.uid == uid) else { return };
        if Arc::ptr_eq(&ds.doc, &before) {
            return;
        }
        let nodes = edited(&before, &ds.doc, ds.page, params, &ds.selection);
        if nodes.is_empty() {
            return;
        }
        let serial = self.agent.as_ref().map_or(0, |a| a.serial).wrapping_add(1);
        self.agent = Some(AgentEdit { doc: uid, page: ds.page, nodes, serial });
    }

    /// The document in front, before an agent's command runs on it (for [`Session::note_agent_edit`]).
    pub fn agent_before(&self) -> Option<(u64, Arc<Document>)> {
        self.ds().ok().map(|d| (d.uid, d.doc.clone()))
    }
}
