/// Port of src-tauri/src/naming.rs. Paths are native-separator strings.
library;

import 'package:path/path.dart' as p;

const String filePrefix = 'RFI No. ';

String filenameFor(String code) => '$filePrefix$code.pdf';

String folderFor(String destination, String code) => p.join(destination, code);

String targetFor(String destination, String code) =>
    p.join(folderFor(destination, code), filenameFor(code));

String flatTargetFor(String destination, String code) =>
    p.join(destination, filenameFor(code));
