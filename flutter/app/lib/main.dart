import 'dart:io';

import 'package:flutter/widgets.dart';

import 'app/doc_flow_app.dart';
import 'app/screen_registry.dart';
import 'engine/lib.dart';
import 'engine/platform/platform_services.dart';
import 'ui/plan/plan.dart';
import 'ui/screens/admin_screen.dart';
import 'ui/screens/history_screen.dart';
import 'ui/screens/home_screen.dart';
import 'ui/screens/login_screen.dart';
import 'ui/screens/operation_screen.dart';
import 'ui/screens/pending_screen.dart';
import 'ui/screens/recovery_screen.dart';
import 'ui/screens/review_screen.dart';
import 'ui/screens/settings_screen.dart';

Future<void> main(List<String> arguments) async {
  final smokeStatus = await maybeRunSmoke(arguments);
  if (smokeStatus != null) exit(smokeStatus);

  WidgetsFlutterBinding.ensureInitialized();
  final screens = ScreenRegistry();
  registerLoginScreen(screens);
  registerPendingScreen(screens);
  registerSettingsScreen(screens);
  registerHomeScreen(screens);
  registerOperationScreen(screens, stages: const AppStageViews());
  registerReviewScreen(screens, planPreview: planPreview);
  registerRecoveryScreen(screens);
  registerHistoryScreen(screens);
  registerAdminScreen(screens);
  runApp(
    DocFlowApp(
      screens: screens,
      engine: buildDocFlowEngine(platform: ProductionPlatformServices()),
    ),
  );
}
