// The preview and result stages handed to the operation screen.

import 'package:flutter/widgets.dart';

import '../screens/operation_screen.dart';
import '../screens/result_view.dart';
import 'plan_view.dart';

/// [OperationStageViews] backed by [PlanView] and [ResultView].
final class AppStageViews implements OperationStageViews {
  const AppStageViews();

  @override
  Widget planView(PlanStageProps props) => PlanView.fromProps(props);

  @override
  Widget resultView(ResultStageProps props) => ResultView(
    result: props.result,
    s: props.s,
    error: props.error,
    onDone: props.onDone,
    onUndo: props.onUndo,
    onResumePrint: props.onResumePrint,
    copySheets: props.copySheets,
    copyEnabled: props.copyEnabled,
    copyingSheet: props.copyingSheet,
    onCopy: props.onCopy,
    onPrepareRfiReply: props.onPrepareRfiReply,
    onOpenDestination: props.onOpenDestination,
  );
}
