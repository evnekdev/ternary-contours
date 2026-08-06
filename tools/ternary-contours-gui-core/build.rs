use std::{collections::BTreeSet, env, fs, path::PathBuf};

#[derive(Clone, Debug)]
struct Element {
    object_name: String,
    qt_class: String,
    source_file: String,
    parent_object_name: String,
    is_public: bool,
}

fn rust_variant(name: &str) -> String {
    let mut output = String::new();
    let mut uppercase = true;
    for character in name.chars() {
        if character == '_' || character == '-' {
            uppercase = true;
        } else if uppercase {
            output.extend(character.to_uppercase());
            uppercase = false;
        } else {
            output.push(character);
        }
    }
    output
}

fn property_text(node: roxmltree::Node<'_, '_>, name: &str) -> Option<String> {
    node.children()
        .find(|child| child.has_tag_name("property") && child.attribute("name") == Some(name))
        .and_then(|property| {
            property
                .descendants()
                .find(|child| child.has_tag_name("string"))
        })
        .and_then(|string| string.text())
        .map(str::to_owned)
}

fn public_widget(class: &str, object_name: &str) -> bool {
    matches!(
        class,
        "QMainWindow"
            | "QMenu"
            | "QMenuBar"
            | "QTabWidget"
            | "QTreeView"
            | "QTableView"
            | "QSplitter"
            | "QStatusBar"
            | "QPushButton"
            | "QDialog"
            | "QDialogButtonBox"
            | "QLineEdit"
            | "QRadioButton"
            | "QSpinBox"
            | "QCheckBox"
            | "TernaryCanvas"
    ) || object_name.starts_with("tab")
}

fn main() {
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let ui_directory = manifest.join("../../apps/ternary-contours-qt/ui");
    // Track the directory itself so adding a new Designer dialog regenerates
    // the authoritative inventory on the next Cargo invocation.
    println!("cargo:rerun-if-changed={}", ui_directory.display());
    let mut paths = fs::read_dir(&ui_directory)
        .expect("Qt Designer UI directory must exist")
        .map(|entry| entry.expect("read UI entry").path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "ui"))
        .collect::<Vec<_>>();
    paths.sort();

    let mut elements = Vec::new();
    let mut tab_order = Vec::new();
    let mut names = BTreeSet::new();
    for path in paths {
        println!("cargo:rerun-if-changed={}", path.display());
        let source = fs::read_to_string(&path).expect("read Qt Designer XML");
        let document = roxmltree::Document::parse(&source).expect("parse Qt Designer XML");
        for node in document
            .descendants()
            .filter(|node| node.has_tag_name("widget"))
        {
            let qt_class = node.attribute("class").unwrap_or_default();
            if matches!(qt_class, "QDialog" | "QWidget") {
                assert!(
                    node.children().any(|child| child.has_tag_name("layout")),
                    "Qt Designer container `{}` must use a managed layout",
                    node.attribute("name").unwrap_or("<unnamed>")
                );
            }
            if node.attribute("name") != Some("mainWindow")
                && property_text(node, "geometry").is_some()
            {
                panic!(
                    "Qt Designer child `{}` must not use fixed absolute geometry",
                    node.attribute("name").unwrap_or("<unnamed>")
                );
            }
        }
        for tabstop in document
            .descendants()
            .filter(|node| node.has_tag_name("tabstop"))
        {
            let object_name = tabstop
                .text()
                .expect("Qt Designer tabstop must name an object")
                .trim();
            assert!(
                !object_name.is_empty(),
                "Qt Designer tabstop cannot be empty: {}",
                path.display()
            );
            tab_order.push(object_name.to_owned());
        }
        for string in document
            .descendants()
            .filter(|node| node.has_tag_name("string"))
        {
            assert!(
                string.attribute("notr") != Some("true"),
                "Qt Designer user-visible strings must remain translatable: {}",
                path.display()
            );
        }
        for node in document
            .descendants()
            .filter(|node| node.has_tag_name("widget") || node.has_tag_name("action"))
        {
            let Some(object_name) = node.attribute("name") else {
                continue;
            };
            if object_name == "separator"
                || object_name == "centralWidget"
                || object_name.ends_with("Spacer")
            {
                continue;
            }
            let property_names = node
                .children()
                .filter(|child| child.has_tag_name("property"))
                .filter_map(|property| property.attribute("name"))
                .collect::<Vec<_>>();
            assert!(
                property_names.iter().collect::<BTreeSet<_>>().len() == property_names.len(),
                "Qt Designer object `{object_name}` declares a property more than once"
            );
            let qt_class = node.attribute("class").unwrap_or("QAction");
            let is_public = node.has_tag_name("action") || public_widget(qt_class, object_name);
            if !names.insert(object_name.to_owned()) {
                panic!("duplicate Qt Designer objectName `{object_name}`");
            }
            if is_public
                && (object_name.starts_with("pushButton")
                    || object_name.starts_with("comboBox")
                    || object_name.starts_with("widget_"))
            {
                panic!("public Qt Designer object uses generic objectName `{object_name}`");
            }
            if is_public && !node.has_tag_name("action") {
                assert!(
                    property_text(node, "accessibleName").is_some(),
                    "public Qt object `{object_name}` is missing accessibleName"
                );
                assert!(
                    property_text(node, "accessibleDescription").is_some(),
                    "public Qt object `{object_name}` is missing accessibleDescription"
                );
            }
            elements.push(Element {
                object_name: object_name.to_owned(),
                qt_class: qt_class.to_owned(),
                source_file: path.file_name().unwrap().to_string_lossy().into_owned(),
                parent_object_name: node
                    .ancestors()
                    .skip(1)
                    .find_map(|ancestor| ancestor.attribute("name"))
                    .unwrap_or("")
                    .to_owned(),
                is_public,
            });
        }
    }
    elements.sort_by(|left, right| left.object_name.cmp(&right.object_name));
    let variants = elements
        .iter()
        .map(|element| rust_variant(&element.object_name))
        .collect::<Vec<_>>();
    if variants.iter().collect::<BTreeSet<_>>().len() != variants.len() {
        panic!("Qt Designer object names do not generate unique Rust identifiers");
    }
    let output = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let mut ids = String::from("// @generated by build.rs from Qt Designer XML. Do not edit.\n");
    ids.push_str("#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]\npub enum QtUiElementId {\n");
    for variant in &variants {
        ids.push_str(&format!("    {variant},\n"));
    }
    ids.push_str("}\n");
    fs::write(output.join("qt_ui_ids.rs"), ids).expect("write generated Qt UI IDs");

    let mut inventory =
        String::from("// @generated by build.rs from Qt Designer XML. Do not edit.\n");
    inventory.push_str("#[derive(Clone, Copy, Debug, Eq, PartialEq)]\npub struct QtUiElementDefinition {\n    pub id: QtUiElementId,\n    pub object_name: &'static str,\n    pub qt_class: &'static str,\n    pub source_file: &'static str,\n    pub parent_object_name: &'static str,\n    pub is_public: bool,\n}\n\npub const QT_UI_ELEMENTS: &[QtUiElementDefinition] = &[\n");
    for (element, variant) in elements.iter().zip(&variants) {
        inventory.push_str(&format!("    QtUiElementDefinition {{ id: QtUiElementId::{variant}, object_name: {:?}, qt_class: {:?}, source_file: {:?}, parent_object_name: {:?}, is_public: {} }},\n", element.object_name, element.qt_class, element.source_file, element.parent_object_name, element.is_public));
    }
    inventory.push_str("];\n");
    fs::write(output.join("qt_ui_inventory.rs"), inventory)
        .expect("write generated Qt UI inventory");

    let mut hierarchy =
        String::from("// @generated by build.rs from Qt Designer XML. Do not edit.\n");
    hierarchy.push_str("pub const QT_UI_HIERARCHY: &[(QtUiElementId, &str)] = &[\n");
    for (element, variant) in elements.iter().zip(&variants) {
        hierarchy.push_str(&format!(
            "    (QtUiElementId::{variant}, {:?}),\n",
            element.parent_object_name
        ));
    }
    hierarchy.push_str("];\n");
    fs::write(output.join("qt_ui_hierarchy.rs"), hierarchy)
        .expect("write generated Qt UI hierarchy");

    let mut actions =
        String::from("// @generated by build.rs from Qt Designer XML. Do not edit.\n");
    actions.push_str("pub const QT_UI_ACTIONS: &[QtUiElementId] = &[\n");
    for (_, variant) in elements
        .iter()
        .zip(&variants)
        .filter(|(element, _)| element.qt_class == "QAction")
    {
        actions.push_str(&format!("    QtUiElementId::{variant},\n"));
    }
    actions.push_str("];\n");
    fs::write(output.join("qt_ui_actions.rs"), actions).expect("write generated Qt UI actions");

    let mut tab_order_source =
        String::from("// @generated by build.rs from Qt Designer XML. Do not edit.\n");
    tab_order_source.push_str("pub const QT_UI_TAB_ORDER: &[QtUiElementId] = &[\n");
    for object_name in &tab_order {
        assert!(
            names.contains(object_name),
            "Qt Designer tabstop `{object_name}` does not name a known object"
        );
        tab_order_source.push_str(&format!(
            "    QtUiElementId::{},\n",
            rust_variant(object_name)
        ));
    }
    tab_order_source.push_str("];\n");
    fs::write(output.join("qt_ui_tab_order.rs"), tab_order_source)
        .expect("write generated Qt UI tab order");
}
