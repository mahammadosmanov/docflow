use super::*;

pub(super) fn file_group_operation() -> OperationDef {
    operation(
        "rfi.group_files",
        vec![
            parameter(
                "source",
                ParamKind::Folder,
                "param.rfi.fileGroup.source",
                Value::Null,
                true,
                Vec::new(),
                BTreeMap::new(),
                Map::new(),
                Some("param.rfi.fileGroup.source.help"),
            ),
            parameter(
                "destination",
                ParamKind::Folder,
                "param.rfi.fileGroup.destination",
                Value::Null,
                true,
                Vec::new(),
                BTreeMap::new(),
                Map::new(),
                Some("param.rfi.fileGroup.destination.help"),
            ),
        ],
    )
}
