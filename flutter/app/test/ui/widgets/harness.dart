// Shared harness for the widget tests: a themed app with a fixed viewport and
// animations disabled, so pumpAndSettle terminates.

import 'package:docflow/l10n/strings.dart';
import 'package:docflow/models/prefs.dart';
import 'package:docflow/theme/app_theme.dart';
import 'package:flutter/material.dart' hide IconButton, Switch;
import 'package:flutter_test/flutter_test.dart';

Strings testStrings() => makeStrings(AppLocale.en);

Future<void> pumpWidgetApp(
  WidgetTester tester,
  Widget home, {
  Size size = const Size(800, 800),
}) async {
  tester.view.devicePixelRatio = 1;
  tester.view.physicalSize = size;
  addTearDown(tester.view.resetPhysicalSize);
  addTearDown(tester.view.resetDevicePixelRatio);
  await tester.pumpWidget(
    MaterialApp(
      theme: buildAppTheme(brightness: Brightness.light),
      builder: (context, child) => MediaQuery(
        data: MediaQuery.of(context).copyWith(disableAnimations: true),
        child: child!,
      ),
      home: Scaffold(body: home),
    ),
  );
}
