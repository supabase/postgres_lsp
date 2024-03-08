// use std::collections::HashMap;
//
// use crate::file_change::FileChange;
//
// struct Pending {}
//
// struct Analysis {
//     pending_db: HashMap<FileId, Pending>,
// }
//
// struct AnalysisHost {}
//
// impl AnalysisHost {
//     fn new() -> AnalysisHost {
//         AnalysisHost {}
//     }
//
//     /// Returns a snapshot of the current state, which you can query for
//     /// semantic information.
//     pub fn analysis(&self) -> Analysis {
//         Analysis {
//             db: self.db.snapshot(),
//         }
//     }
//
//     /// Applies changes to the current state of the world. If there are
//     /// outstanding snapshots, they will be canceled.
//     pub fn apply_change(&mut self, change: FileChange) {
//         self.db.apply_change(change);
//     }
// }

// problem: we do not snapshot but immediately apply changes and resolve requests
// no cancellation of requests like in rust-analyzer
//
//
//
// biome service: also has extension stuff figured out
// we just have to bring it down to the statement level
// so instead of documents, we store statements
//
// sinilar to biome: they store results on workspace server per file eg syntax tree
// we do the same: store results per statement on workspace level. on change, just clear it.
// we also store statement features and then just fetch them on demand and cache them.
// a feature can also be an extension and is based on
// features are stored on app level
// and then we get capabilities for a feature for a given statement
// features o
//
// we first get parse results and if yes call feature o
// parse results are cached
// if no we return
//
// only problem: they have a common representation for a syntax tree shared between all languages
// so that eg analyze inputs is language agnostic
//
// for us, tstree is the same for all languages
//
// BUT anything else is different for every language
// but maybe it still works. we will always have some tree and diagnostics for every language
// parser
//
// we do not have an unified data source for all
// its different for every language.
//
// we will most likely have to carry around the language with the data
// so we wont have a common crate for every operation
//
// eg plpgsql_analyze
// must be called with plpgsql data
//
// we store features outside of statement data
//
// instead of just syntax, we store multiple syntax, one for each language
//
// biome simply updates the entire file by taking the changes and applying them to the entire file
// we have to allow individual changes to a file to figure out what statements need to be updated
//
// gameplan:
// - create workspace and app structs
// - just document apis
// - then find a solution for parse results and extensions
