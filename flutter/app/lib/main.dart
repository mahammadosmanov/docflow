import 'package:flutter/widgets.dart';

import 'app/doc_flow_app.dart';
import 'app/screen_registry.dart';

void main() {
  WidgetsFlutterBinding.ensureInitialized();
  // Screen owners register their builders here, before runApp:
  //   registerHomeScreen(screens);
  // The engine is supplied as DocFlowApp(engine: ...) once the engine
  // composition root exists.
  final screens = ScreenRegistry();
  runApp(DocFlowApp(screens: screens));
}
