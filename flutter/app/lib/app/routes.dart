// Navigation state (`Route` in App.tsx) and screen identifiers.

import '../models/models.dart';

/// Every screen the shell can show. Login and Pending are chosen from the auth
/// state; the others are navigation routes. Screen owners register a builder
/// for each id in the ScreenRegistry.
enum ScreenId {
  login,
  pending,
  settings,
  home,
  operation,
  review,
  recovery,
  history,
  admin,
}

/// Where Settings returns to (`back` in the settings route).
enum SettingsBack { home, login, pending }

/// A navigation destination inside the signed-in workspace.
sealed class AppRoute {
  const AppRoute();

  ScreenId get screen;
}

final class HomeRoute extends AppRoute {
  const HomeRoute();

  @override
  ScreenId get screen => ScreenId.home;
}

final class OperationRoute extends AppRoute {
  const OperationRoute({
    required this.operation,
    this.initialValues,
    this.initialPlan,
  });

  final OperationDef operation;
  final Map<String, Object?>? initialValues;
  final Plan? initialPlan;

  @override
  ScreenId get screen => ScreenId.operation;
}

final class ReviewRoute extends AppRoute {
  const ReviewRoute({required this.destination});

  final PathSelection destination;

  @override
  ScreenId get screen => ScreenId.review;
}

final class RecoveryRoute extends AppRoute {
  const RecoveryRoute();

  @override
  ScreenId get screen => ScreenId.recovery;
}

final class HistoryRoute extends AppRoute {
  const HistoryRoute();

  @override
  ScreenId get screen => ScreenId.history;
}

final class AdminRoute extends AppRoute {
  const AdminRoute();

  @override
  ScreenId get screen => ScreenId.admin;
}

final class SettingsRoute extends AppRoute {
  const SettingsRoute({required this.back});

  final SettingsBack back;

  @override
  ScreenId get screen => ScreenId.settings;
}
