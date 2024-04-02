// mod analysis_host;
// mod file_change;
//
// use dashmap::DashMap;
// use schema_cache::SchemaCache;
// use tree_sitter::Tree;
//
// fn tmp() {
//     println!("Hello, world!");
// }
//
// struct WorldState {
//     statements: DashMap<StatementLocation, IDE>,
// }
//
// // since the data model is differnet for every state we need to implement the api for every state
// //
// // for parsed and ready, we maybe even need to implement the api for every node type?
// // we should think about a solution that does not require this
// // maybe a deeper analysis on what and hwo to resolve?
// // is it worth it to produce a HIR for every node type?
// //
// // relevant features from lsp
// // NOT go to... only in plpgsql etc
// // NOT find refs... only in plpgsql etc
// // NOT call hierarchy stuff bc it returns where something is used
// // NOT type hierarchy stuff bc it returns where something is used
// // document highlight --> only in plpgsql
// // hover -> YES: just get data from schema cache -> ts, cst
// // code lenses -> YES: e.g. run() on top of statements. -> AST
// // folding range request -> YES, folding range is where the code can be folded -> CST
// // Selection Range Request -> YES, shows enclosing area of the selection, e.g. statement or sub
// // statement -> CST
// // document symbols -> yes, but only for plpgsql and maybe also just per statement -> stmt
// // semantic tokens -> yes -> ts, cst
// // inline value request -> only in plpgsql
// // inlay hint: yes, e.g. for functions -> ts, cst
// // completion: yes -> ts
// // diagnostics: yes -> AST
// // signature help: yes -> ts, cst
// // code action: yes eg execute statement -> stmt, maybe other stuff later eg from linter then AST
// // formatting: yes (full doc, range and maybe while typing) -> CST, maybe later AST
// // rename: no, just in plgpsql
// //
// // per node type only relevant for ast. and ast is only relevant for some features that require
// // per node logic anyway eg linter
// //
// // "analysis" not really required for most features. just for linter, type checking etc so
// // basically diagnostics and code actions / assists.
// // just cst should be build in background and then used for the features but only bc its built from
// // the ast
// //
// // change -> ts direct. then bg: cst, ast, then analysis
// // type checks, linter etc handlers get cst, ast and schema cache
// // cst and ast is kicked off directly with every change. bg analysis is kicked off if there is no
// // syntax error from the ast and when the user saves a file
// //
// //
// // only thing left are plpgsql / plrust / plpython / plv8 functions
// // --> we can use rust libs for each language!! eg rust analyzer, oxc for node, ruff for python!
// // --> for plpgsql we use the api from libpg_query and parse the json into our own data model
// //
// // all of this should be implemented in their own modules
// //
// // now only thing left is to design how we implement the api for different states
// //
// // just fns that define the required states via input
// //
// // IDE is a simple wrapper that holds the data and passes a ref to fns
//
// // some apis require per-file processing too
// // --> not really, e.g. for folding range we just use the range of the statement
//
// type FileId = u32;
//
// type StatementIdx = u32;
//
// #[derive(Debug, Hash)]
// struct StatementLocation {
//     file_id: FileId,
//     statement_idx: StatementIdx,
// }
//
// struct Pending {
//     tree: Tree,
// }
//
// // this has to be extensble to support e.g. plgpsql and plpython etc
// struct Working {}
//
// // we might want the states itself also be extensible?
// struct Snapshot {}
//
// struct Parsed {}
//
// // the data in snapshot could be different for different nodes?
//
// struct AnalysisHost {
//     // versioning will be important
//     version: u32,
//
//     // sync update on tree sitter
//     pending: Pending,
//
//     // report diagnostics back somehow -> via channel?
//     // rust analyser stores the files for which there were changes, and then gets the diagnostics
//     // for the files as a task
//     //
//     // we will produce diagnostics directly and store them in Ready
//     parsed: Parsed,
//
//     //
//     ready: Snapshot,
// }
//
// impl AnalysisHost {
//     fn new() -> AnalysisHost {
//         AnalysisHost {
//             version: 0,
//             pending: Pending { tree: Tree::new() },
//             parsed: Parsed {},
//             ready: Snapshot {},
//         }
//     }
// }
//
// enum ExtensionType {
//     PlPgSQL,
//     PlRust,
//     PlPython,
//     PlV8,
// }
//
// struct IDEExtension {}
//
// // we need more info on if soemting is stale / processing is beind done right now etc
//
// struct IDE {
//     source: String,
//     pub range: TextRange,
//     schema_cache: SchemaCache,
//
//     // there can be just one extension per statement (e.g. plpgsql, plrust, plpython, plv8)
//     extension: IDEExtension,
//
//     analysis: AnalysisHost,
// }
//
// impl IDE {
//     pub fn new(source: String, schema_cache: SchemaCache) -> IDE {
//         IDE {
//             source,
//             schema_cache,
//             analysis: AnalysisHost::new(),
//         }
//     }
//
//     pub fn update_schema_cache(&mut self) {
//         // update schema cache
//         // kick off analysis again
//     }
//
//     // if other statements are updated, we just need to update the range of this one
//     pub fn update_range(&mut self, range: TextRange) {
//         self.range = range;
//     }
// }
//
// #[cfg(test)]
// mod tests {
//     use std::sync::mpsc::channel;
//
//     #[test]
//     fn test_playground() {
//         // every ide gets its own sender
//         let (tx, rx) = channel();
//
//         // we push updates synchronously, and the IDE then kicks off parallel processing in the
//         // background itself
//         // upon completion, we send the result back to the IDE
//         // but then we enforce threading from the outside
//
//         // OR:
//         // all of ide is does not use any async or threading at all
//         // instead it exposed an api to the outside that can be used to kick off processing
//     }
// }
//
// struct StatementAnalysis {
//     // implements api
//     // gets snapshot of db
// }
//
// struct StatementDatabase {
//     // holds data
// }
//
// struct AnalysisHost1 {
//     // global
//     // exposes analysis snapshot
//     // apply change
//     // holds db for stmt
// }
//
// struct Pending1 {
//     // holds ts tree
//     // we do not create a new one, just keep this one and update it
//     // we then implement some ide api methods on pending
// }
//
// struct Ready1 {
//     // holds ast
//     // also spawns extensions if required
//     // implements api
// }

// i dont want to expose file management to api user
// so we will have a wrapper that contls file and statement handling
