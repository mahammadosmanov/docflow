// OperationScreen from frontend/src/App.tsx: the form, preview, apply and
// result flow of one operation. Forms are generic over the engine's ParamSchema.
//
// The preview and result stages are PlanView and ResultView, owned by their
// own screen ports. They are supplied through [OperationStageViews] when the
// screen is registered, so this file never depends on their internals.

import 'dart:async';
import 'dart:convert';

import 'package:flutter/material.dart' hide IconButton, Switch;
import 'package:flutter/services.dart';

import '../../app/app_utils.dart';
import '../../app/human_error.dart';
import '../../app/routes.dart';
import '../../app/screen_registry.dart';
import '../../bridge/bridge.dart';
import '../../bridge/platform.dart';
import '../../l10n/strings.dart';
import '../../models/models.dart';
import '../../theme/app_icons.dart';
import '../../theme/tokens.dart';
import '../../theme/typography.dart';
import '../widgets/metrics.dart';
import '../widgets/widgets.dart';
import 'operation_catalog.dart';
import 'preset_toolbar.dart';
import 'schema_field.dart';

/// Everything the preview stage (PlanView) receives (App.tsx `PlanView` props).
final class PlanStageProps {
  const PlanStageProps({
    required this.plan,
    required this.s,
    required this.error,
    required this.onCancel,
    required this.onApply,
    required this.editorRows,
    required this.onEditorRowsChanged,
    required this.editorDirty,
    required this.onUpdatePreview,
    required this.isUpdatingPreview,
    required this.scanPreviewUpdated,
  });

  final Plan plan;
  final Strings s;
  final String error;
  final VoidCallback onCancel;
  final VoidCallback onApply;
  final List<Map<String, Object?>> editorRows;
  final ValueChanged<List<Map<String, Object?>>> onEditorRowsChanged;
  final bool editorDirty;
  final VoidCallback onUpdatePreview;
  final bool isUpdatingPreview;
  final bool scanPreviewUpdated;
}

/// Everything the result stage (ResultView) receives (App.tsx `ResultView`
/// props). A null callback means the action is not offered.
final class ResultStageProps {
  const ResultStageProps({
    required this.result,
    required this.s,
    required this.error,
    required this.onDone,
    required this.onUndo,
    required this.onResumePrint,
    required this.copySheets,
    required this.copyEnabled,
    required this.copyingSheet,
    required this.onCopy,
    required this.onPrepareRfiReply,
    required this.onOpenDestination,
  });

  final OperationResult result;
  final Strings s;
  final String error;
  final VoidCallback onDone;
  final VoidCallback onUndo;
  final VoidCallback? onResumePrint;
  final List<String> copySheets;
  final bool copyEnabled;

  /// The sheet being copied: '' for the whole table, null when idle.
  final String? copyingSheet;

  /// Copies one sheet, or the whole table when the argument is null.
  final ValueChanged<String?> onCopy;
  final VoidCallback? onPrepareRfiReply;
  final VoidCallback? onOpenDestination;
}

/// The two stages of an operation that other screen ports own.
abstract interface class OperationStageViews {
  /// PlanView: the approved preview with its editor and apply action.
  Widget planView(PlanStageProps props);

  /// ResultView: the outcome of an applied plan.
  Widget resultView(ResultStageProps props);
}

/// Registers the operation screen (`ScreenId.operation`). The composition root
/// passes the preview and result stages it has built.
void registerOperationScreen(
  ScreenRegistry registry, {
  required OperationStageViews stages,
}) {
  registry.register(ScreenId.operation, (screen) {
    final route = screen.route;
    if (route is! OperationRoute) {
      throw StateError('The operation screen needs an OperationRoute');
    }
    return OperationScreen(screen: screen, route: route, stages: stages);
  });
}

enum _Stage { form, previewing, preview, applying, result }

/// Runs one operation: configure, preview, apply, and review the result.
class OperationScreen extends StatefulWidget {
  const OperationScreen({
    super.key,
    required this.screen,
    required this.route,
    required this.stages,
  });

  final ScreenContext screen;
  final OperationRoute route;
  final OperationStageViews stages;

  @override
  State<OperationScreen> createState() => _OperationScreenState();
}

/// Parameter values as the engine exchanges them (path values as JSON maps).
bool _isPersistableParam(ParamDef? param) =>
    param == null ||
    (param.remember != false &&
        param.kind != ParamKind.longText &&
        param.kind != ParamKind.secret);

/// Serializes [values] with sorted keys, so equal content compares equal.
String _canonicalJson(Map<String, Object?> values) {
  final keys = values.keys.toList()..sort();
  return jsonEncode({for (final key in keys) key: values[key]});
}

RfiPrintCandidate _candidateWithSelection(
  RfiPrintCandidate candidate,
  bool selected,
) => RfiPrintCandidate(
  requested: candidate.requested,
  rfiCode: candidate.rfiCode,
  documentType: candidate.documentType,
  displayPath: candidate.displayPath,
  selection: candidate.selection,
  selected: selected,
);

final RegExp _disciplinePattern = RegExp(r'-RFI-([A-Z0-9]+)-');

const List<String> _sourceKeysForDestination = [
  'destination',
  'dest',
  'target',
  'output_directory',
  'rfi_system',
];

class _OperationScreenState extends State<OperationScreen> {
  late Map<String, Object?> _values;
  final Map<String, Object?> _defaults = {};
  int _valuesRevision = 0;
  bool _hydrated = false;
  String _lastSaved = '';
  Timer? _saveTimer;

  _Stage _stage = _Stage.form;
  Plan? _plan;
  OperationResult? _result;
  ProgressEvent? _progress;
  List<ProgressEvent> _progressTrail = const [];
  String _error = '';
  List<Map<String, Object?>> _editorRows = const [];
  bool _editorDirty = false;

  List<RfiPrintCandidate> _rfiCandidates = const [];
  List<RfiPrintResolution> _rfiResolutions = const [];
  bool _rfiFinderBusy = false;
  String _rfiFinderError = '';
  List<String> _excludedRfiInputs = const [];
  bool _rfiFinderWasRun = false;
  int _rfiSearchRevision = 0;
  bool _rfiSearchTruncated = false;

  bool _isUpdatingScanPreview = false;
  bool _scanPreviewUpdated = false;
  String? _copyingSheet;
  PrintQueueSummary? _latestPrintQueue;
  Map<String, Object?>? _previousValues;
  bool _cancelling = false;

  OperationDef get _operation => widget.route.operation;

  Strings get _s => widget.screen.strings;

  Bridge get _bridge => widget.screen.bridge;

  ParamDef? _param(String name) {
    for (final param in _operation.schema) {
      if (param.name == name) return param;
    }
    return null;
  }

  List<String> get _rfiInputs => _operation.id == 'files.batch_print'
      ? parseRfiInputs(_values['rfi_numbers'])
      : const [];

  @override
  void initState() {
    super.initState();
    for (final param in _operation.schema) {
      _defaults[param.name] = param.defaultValue;
    }
    _values = {..._defaults, ...?widget.route.initialValues};
    if (widget.route.initialPlan != null) {
      _plan = widget.route.initialPlan;
      _stage = _Stage.preview;
    }
    HardwareKeyboard.instance.addHandler(_onKeyEvent);
    unawaited(_hydrate());
    if (_operation.id == 'files.batch_print') {
      unawaited(_loadLatestPrintQueue());
    }
  }

  @override
  void dispose() {
    HardwareKeyboard.instance.removeHandler(_onKeyEvent);
    _saveTimer?.cancel();
    super.dispose();
  }

  /// Ctrl or Cmd with Enter moves the flow forward; Escape steps it back. The
  /// listener ignores keys while another route (a dialog) is on top.
  bool _onKeyEvent(KeyEvent event) {
    if (event is KeyUpEvent || !mounted) return false;
    final route = ModalRoute.maybeOf(context);
    if (route != null && !route.isCurrent) return false;
    final keyboard = HardwareKeyboard.instance;
    final key = event.logicalKey;
    if ((keyboard.isControlPressed || keyboard.isMetaPressed) &&
        (key == LogicalKeyboardKey.enter ||
            key == LogicalKeyboardKey.numpadEnter)) {
      if (_stage == _Stage.form) {
        unawaited(_preview());
        return true;
      }
      if (_stage == _Stage.preview && !_cancelling) {
        unawaited(_apply());
        return true;
      }
      return false;
    }
    if (key == LogicalKeyboardKey.escape) {
      if (_stage == _Stage.previewing) {
        unawaited(_cancelPreview());
        return true;
      }
      if (_stage == _Stage.preview || _stage == _Stage.result) {
        _setStage(_Stage.form);
        return true;
      }
      if (_stage == _Stage.form) {
        _goBack();
        return true;
      }
    }
    return false;
  }

  void _goBack() => widget.screen.goHome();

  void _setStage(_Stage stage) {
    if (mounted) setState(() => _stage = stage);
  }

  void _showError(String message) {
    if (mounted) setState(() => _error = message);
  }

  /// Keeps only the values that may be stored between sessions.
  Map<String, Object?> _persistable(Map<String, Object?> values) => {
    for (final entry in values.entries)
      if (_isPersistableParam(_param(entry.key))) entry.key: entry.value,
  };

  Future<void> _hydrate() async {
    final revision = ++_valuesRevision;
    try {
      final saved = await _bridge.call(
        FrontendCommands.settingsParams,
        params: (opId: _operation.id),
      );
      if (!mounted) return;
      if (_valuesRevision != revision) {
        setState(() => _hydrated = true);
        return;
      }
      final filtered = <String, Object?>{
        for (final entry in saved.entries)
          if (_param(entry.key) case final param?
              when param.remember != false)
            entry.key: entry.value,
      };
      final initial = widget.route.initialValues ?? const {};
      setState(() {
        _previousValues = _operation.supportsPresets && filtered.isNotEmpty
            ? filtered
            : null;
        _values = {
          ..._values,
          if (!_operation.supportsPresets) ...filtered,
          ...initial,
        };
        _lastSaved = _canonicalJson(
          _persistable({..._defaults, ...filtered, ...initial}),
        );
        _hydrated = true;
      });
      _scheduleSave();
    } catch (_) {
      if (mounted) setState(() => _hydrated = true);
    }
  }

  /// Saves safe values 200 ms after the last change, for operations that do not
  /// use presets (the React effect on `values`).
  void _scheduleSave() {
    _saveTimer?.cancel();
    if (!_hydrated || _operation.supportsPresets) return;
    final sanitized = _persistable(_values);
    final serialized = _canonicalJson(sanitized);
    if (serialized == _lastSaved) return;
    _saveTimer = Timer(const Duration(milliseconds: 200), () {
      _lastSaved = serialized;
      unawaited(
        _bridge
            .call(
              FrontendCommands.settingsSaveParams,
              params: (opId: _operation.id, values: sanitized),
            )
            .then<void>((_) {}, onError: (Object _) {}),
      );
    });
  }

  void _setValue(String name, Object? value) {
    _valuesRevision++;
    setState(() {
      _values = {..._values, name: value};
      if (_operation.id == 'files.batch_print' &&
          const {
            'rfi_system_root',
            'rfi_numbers',
            'include_first_page',
            'include_sketch',
          }.contains(name)) {
        _invalidateRfiFinder();
      }
      if (name == 'rfi_numbers') _excludedRfiInputs = const [];
    });
    _scheduleSave();
  }

  void _invalidateRfiFinder() {
    _rfiSearchRevision++;
    _rfiFinderBusy = false;
    _rfiFinderError = '';
    _rfiFinderWasRun = false;
    _rfiSearchTruncated = false;
    _rfiCandidates = const [];
    _rfiResolutions = const [];
  }

  Future<void> _loadLatestPrintQueue() async {
    try {
      final latest = await _bridge.call(FrontendCommands.enginePrintQueueLatest);
      if (mounted) setState(() => _latestPrintQueue = latest);
    } catch (_) {
      if (mounted) setState(() => _latestPrintQueue = null);
    }
  }

  void _receiveProgress(ProgressEvent event) {
    if (!mounted) return;
    final next = [..._progressTrail];
    if (next.isNotEmpty && next.last.phase == event.phase) {
      next[next.length - 1] = event;
    } else {
      next.add(event);
    }
    setState(() {
      _progress = event;
      _progressTrail = next.length > 8
          ? next.sublist(next.length - 8)
          : next;
    });
  }

  void _installPlan(Plan next) {
    final editor = next.meta['editor'];
    final rows = editor is Map && editor['rows'] is List
        ? [
            for (final row in editor['rows'] as List)
              if (row is Map) {...row.cast<String, Object?>()},
          ]
        : <Map<String, Object?>>[];
    setState(() {
      _plan = next;
      _editorRows = rows;
      _editorDirty = false;
      _scanPreviewUpdated = false;
      _stage = _Stage.preview;
    });
  }

  Future<void> _searchRfiDocuments() async {
    final s = _s;
    setState(() {
      _rfiFinderError = '';
      _rfiFinderWasRun = false;
      _rfiCandidates = const [];
      _rfiResolutions = const [];
      _rfiSearchTruncated = false;
    });
    final revision = ++_rfiSearchRevision;
    final root = asPathSelection(_values['rfi_system_root']);
    if (root == null) {
      setState(() => _rfiFinderError = s.t('print.rfi.rootRequired'));
      return;
    }
    final numbers = [
      for (final input in _rfiInputs)
        if (!_excludedRfiInputs.contains(input)) input,
    ];
    if (numbers.isEmpty) {
      setState(() => _rfiFinderError = s.t('print.rfi.inputRequired'));
      return;
    }
    if (_values['include_first_page'] != true &&
        _values['include_sketch'] != true) {
      setState(() => _rfiFinderError = s.t('print.rfi.typesRequired'));
      return;
    }
    setState(() => _rfiFinderBusy = true);
    try {
      final result = await _bridge.call(
        FrontendCommands.rfiPrintFind,
        params: (
          root: root,
          numbers: numbers,
          firstPage: _values['include_first_page'] == true,
          sketch: _values['include_sketch'] == true,
        ),
      );
      if (!mounted || revision != _rfiSearchRevision) return;
      setState(() {
        _rfiCandidates = result.candidates;
        _rfiResolutions = result.resolutions;
        _rfiFinderWasRun = true;
        _rfiSearchTruncated = result.truncated;
      });
    } catch (reason) {
      if (mounted && revision == _rfiSearchRevision) {
        setState(() => _rfiFinderError = humanError(reason, s));
      }
    } finally {
      if (mounted && revision == _rfiSearchRevision) {
        setState(() => _rfiFinderBusy = false);
      }
    }
  }

  void _setCandidateGroup(
    bool Function(RfiPrintCandidate candidate) predicate,
    bool selected,
  ) {
    setState(() {
      _rfiCandidates = [
        for (final candidate in _rfiCandidates)
          predicate(candidate)
              ? _candidateWithSelection(candidate, selected)
              : candidate,
      ];
    });
  }

  Future<void> _preview() async {
    final s = _s;
    setState(() {
      _error = '';
      _progress = const ProgressEvent(phase: 'preparing', done: 0, total: 0);
      _progressTrail = const [
        ProgressEvent(phase: 'preparing', done: 0, total: 0),
      ];
      _stage = _Stage.previewing;
    });
    try {
      final sanitized = _persistable(_values);
      _lastSaved = _canonicalJson(sanitized);
      await _bridge.call(
        FrontendCommands.settingsSaveParams,
        params: (opId: _operation.id, values: sanitized),
      );
      if (_operation.supportsPresets && sanitized.isNotEmpty && mounted) {
        setState(() => _previousValues = sanitized);
      }
      final previewValues = {..._values};
      if (_operation.id == 'files.batch_print') {
        previewValues
          ..remove('rfi_system_root')
          ..remove('rfi_numbers')
          ..remove('include_first_page')
          ..remove('include_sketch');
        if (_rfiFinderWasRun) {
          previewValues['files'] = [
            for (final candidate in _rfiCandidates)
              if (candidate.selected) candidate.selection.toJson(),
          ];
        }
      }
      final next = await _bridge.call(
        FrontendCommands.enginePreview,
        params: (opId: _operation.id, values: previewValues),
        onProgress: _receiveProgress,
      );
      if (mounted) _installPlan(next);
    } catch (reason) {
      if (!mounted) return;
      setState(() {
        _error = humanError(reason, s);
        _stage = _Stage.form;
      });
    }
  }

  Future<void> _cancelPreview() async {
    if (_cancelling) return;
    setState(() => _cancelling = true);
    try {
      await _bridge.call(
        FrontendCommands.engineCancel,
        params: (planId: null, opId: _operation.id),
      );
    } catch (_) {
      // Ignored if the preview already completed or failed.
    } finally {
      if (mounted) {
        setState(() {
          _cancelling = false;
          _stage = _Stage.form;
        });
      }
    }
  }

  Future<void> _resumePrintQueue(String queueId) async {
    final s = _s;
    setState(() {
      _error = '';
      _stage = _Stage.previewing;
    });
    try {
      final plan = await _bridge.call(
        FrontendCommands.enginePrintQueueResume,
        params: (queueId: queueId),
      );
      if (mounted) _installPlan(plan);
    } catch (reason) {
      if (!mounted) return;
      setState(() {
        _error = humanError(reason, s);
        _stage = _Stage.form;
      });
    }
  }

  Future<void> _discardPrintQueue(String queueId) async {
    final s = _s;
    setState(() => _error = '');
    try {
      await _bridge.call(
        FrontendCommands.enginePrintQueueDiscard,
        params: (queueId: queueId),
      );
      if (mounted) setState(() => _latestPrintQueue = null);
    } catch (reason) {
      _showError(humanError(reason, s));
    }
  }

  Future<void> _cancelApply() async {
    final plan = _plan;
    if (plan == null || _cancelling) return;
    setState(() => _cancelling = true);
    try {
      await _bridge.call(
        FrontendCommands.engineCancel,
        params: (planId: plan.planId, opId: null),
      );
    } catch (reason) {
      if (mounted) setState(() => _error = humanError(reason, _s));
    }
  }

  Future<void> _updateScanPreview() async {
    final plan = _plan;
    if (plan == null ||
        plan.opId != primaryScanOperationId ||
        !_editorDirty ||
        _isUpdatingScanPreview) {
      return;
    }
    setState(() {
      _isUpdatingScanPreview = true;
      _error = '';
    });
    try {
      final revised = await _bridge.call(
        FrontendCommands.engineRevise,
        params: (planId: plan.planId, rows: _editorRows),
      );
      if (!mounted) return;
      _installPlan(revised);
      setState(() => _scanPreviewUpdated = true);
    } catch (reason) {
      if (mounted) setState(() => _error = humanError(reason, _s));
    } finally {
      if (mounted) setState(() => _isUpdatingScanPreview = false);
    }
  }

  Future<void> _apply() async {
    if (_editorDirty && _plan?.opId == primaryScanOperationId) {
      await _updateScanPreview();
      return;
    }
    final plan = _plan;
    if (plan == null) return;
    final s = _s;
    setState(() {
      _error = '';
      _progress = const ProgressEvent(phase: 'apply', done: 0, total: 0);
      _progressTrail = const [
        ProgressEvent(phase: 'apply', done: 0, total: 0),
      ];
    });
    try {
      var approvedPlan = plan;
      if (_editorDirty) {
        final previousWarnings = {
          for (final warning in plan.warnings)
            '${warning.key}\u0000${warning.subject}',
        };
        final revised = await _bridge.call(
          FrontendCommands.engineRevise,
          params: (planId: plan.planId, rows: _editorRows),
        );
        if (!mounted) return;
        final revisedEditor = revised.meta['editor'];
        final revisedRows =
            revisedEditor is Map && revisedEditor['rows'] is List
            ? [
                for (final row in revisedEditor['rows'] as List)
                  if (row is Map) {...row.cast<String, Object?>()},
              ]
            : [for (final row in _editorRows) {...row}];
        setState(() {
          _plan = revised;
          _editorRows = revisedRows;
          _editorDirty = false;
        });
        approvedPlan = revised;
        final introduced = revised.warnings.any(
          (warning) => !previousWarnings.contains(
            '${warning.key}\u0000${warning.subject}',
          ),
        );
        if (introduced) {
          setState(() {
            _error = s.t('rfi.editor.reviewWarnings');
            _stage = _Stage.preview;
          });
          return;
        }
      }
      setState(() => _stage = _Stage.applying);
      final result = await _bridge.call(
        FrontendCommands.engineApply,
        params: (opId: _operation.id, planId: approvedPlan.planId),
        onProgress: _receiveProgress,
      );
      if (!mounted) return;
      setState(() {
        _result = result;
        _cancelling = false;
        _stage = _Stage.result;
      });
    } catch (reason) {
      if (!mounted) return;
      setState(() {
        _cancelling = false;
        _error = humanError(reason, s);
        _stage = _Stage.preview;
      });
    }
  }

  Future<void> _copyTable(String? sheet) async {
    final plan = _plan;
    if (plan == null || _copyingSheet != null) return;
    setState(() {
      _copyingSheet = sheet ?? '';
      _error = '';
    });
    try {
      final payload = await _bridge.call(
        FrontendCommands.engineCopyTable,
        params: (planId: plan.planId, sheet: sheet),
      );
      _showError(_s.t('rfi.editor.copied', {'count': payload.count}));
    } catch (reason) {
      _showError(humanError(reason, _s));
    } finally {
      if (mounted) setState(() => _copyingSheet = null);
    }
  }

  /// Requests the Outlook reply preview for the completed log entry. The
  /// React shell passes a no-op for this callback, so the preview is requested
  /// and not displayed, as there.
  Future<void> _prepareRfiReply() async {
    final plan = _plan;
    final journalId = _result?.journalId ?? '';
    if (plan == null || journalId.isEmpty) return;
    setState(() => _error = '');
    try {
      await _bridge.call(
        FrontendCommands.engineRfiReplyPreview,
        params: (planId: plan.planId, journalId: journalId),
      );
    } catch (reason) {
      _showError(humanError(reason, _s));
    }
  }

  Future<void> _undo() async {
    final journalId = _result?.journalId ?? '';
    if (journalId.isEmpty) return;
    try {
      await _bridge.call(
        FrontendCommands.engineUndo,
        params: (journalId: journalId, cloudId: null, destination: null),
      );
      if (mounted) _goBack();
    } catch (reason) {
      _showError(humanError(reason, _s));
    }
  }

  Future<void> _reviewFolder() async {
    try {
      final destination = await chooseFolder(
        _bridge,
        'rfi.review',
        'destination',
      );
      if (mounted && destination != null) {
        widget.screen.navigate(ReviewRoute(destination: destination));
      }
    } catch (reason) {
      _showError(humanError(reason, _s));
    }
  }

  Future<void> _openDestination() async {
    final destination = _firstDestination();
    if (destination == null) return;
    try {
      await revealPath(_bridge, destination.selectionId);
    } catch (reason) {
      _showError(humanError(reason, _s));
    }
  }

  PathSelection? _firstDestination() {
    for (final key in _sourceKeysForDestination) {
      final path = asPathSelection(_values[key]);
      if (path != null) return path;
    }
    return null;
  }

  @override
  Widget build(BuildContext context) {
    final s = _s;
    final op = _operation;
    final stepIndex = switch (_stage) {
      _Stage.form => 0,
      _Stage.previewing || _Stage.preview => 1,
      _Stage.applying || _Stage.result => 2,
    };
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Header(
          title: s.t(op.titleKey),
          onBack: _goBack,
          backLabel: s.t('action.back'),
          bottom: StepIndicator(
            current: stepIndex,
            labels: [
              s.t('step.form'),
              s.t('step.preview'),
              s.t('step.apply'),
            ],
          ),
        ),
        Expanded(
          child: Padding(
            padding: const EdgeInsets.fromLTRB(16, 0, 16, 16),
            child: switch (_stage) {
              _Stage.form => _buildForm(context),
              _Stage.previewing => _buildBusy(
                label: s.t('action.preview'),
                cancelling: _cancelling,
                cancelLabel: s.t(
                  _cancelling ? 'operation.cancelling' : 'action.cancel',
                ),
                onCancel: _cancelPreview,
              ),
              _Stage.preview => _buildPlan(),
              _Stage.applying => _buildBusy(
                label: s.t('plan.applying'),
                cancelling: _cancelling,
                cancelLabel: s.t(
                  _cancelling
                      ? 'operation.cancelling'
                      : op.id == 'files.batch_print'
                      ? 'print.cancelQueue'
                      : 'operation.cancel',
                ),
                onCancel: _cancelApply,
              ),
              _Stage.result => _buildResult(),
            },
          ),
        ),
      ],
    );
  }

  Widget _buildBusy({
    required String label,
    required bool cancelling,
    required String cancelLabel,
    required Future<void> Function() onCancel,
  }) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Expanded(
          child: SingleChildScrollView(
            child: BusyState(
              label: label,
              s: _s,
              progress: _progress,
              progressTrail: _progressTrail,
            ),
          ),
        ),
        ActionBar(
          left: [
            DfButton(
              variant: DfButtonVariant.danger,
              icon: Icon(
                cancelling ? AppIcons.loaderCircle : AppIcons.xCircle,
                size: remPx(context, 0.95),
              ),
              onPressed: cancelling ? null : () => unawaited(onCancel()),
              child: Text(cancelLabel),
            ),
          ],
          right: const [],
        ),
      ],
    );
  }

  Widget _buildPlan() {
    final plan = _plan;
    if (plan == null) return const SizedBox.shrink();
    return Column(
      children: [
        Expanded(
          child: widget.stages.planView(
            PlanStageProps(
              plan: plan,
              s: _s,
              error: _error,
              onCancel: () => _setStage(_Stage.form),
              onApply: () => unawaited(_apply()),
              editorRows: _editorRows,
              onEditorRowsChanged: (rows) => setState(() {
                _editorRows = rows;
                _editorDirty = true;
              }),
              editorDirty: _editorDirty,
              onUpdatePreview: () => unawaited(_updateScanPreview()),
              isUpdatingPreview: _isUpdatingScanPreview,
              scanPreviewUpdated: _scanPreviewUpdated,
            ),
          ),
        ),
      ],
    );
  }

  Widget _buildResult() {
    final result = _result;
    if (result == null) return const SizedBox.shrink();
    final op = _operation;
    final plan = _plan;
    return Column(
      children: [
        Expanded(
          child: widget.stages.resultView(
            ResultStageProps(
              result: result,
              s: _s,
              error: _error,
              onDone: _goBack,
              onUndo: () => unawaited(_undo()),
              onResumePrint:
                  op.id == 'files.batch_print' &&
                      result.meta?['resumeAvailable'] == true
                  ? () => unawaited(
                      _resumePrintQueue(
                        '${result.meta?['queueId'] ?? ''}',
                      ),
                    )
                  : null,
              copySheets: copySheetsFromPlan(plan),
              copyEnabled: planCopyEnabled(plan),
              copyingSheet: _copyingSheet,
              onCopy: (sheet) => unawaited(_copyTable(sheet)),
              onPrepareRfiReply:
                  op.id == 'rfi.log_add' &&
                      result.applied > 0 &&
                      result.failed == 0 &&
                      result.meta?['resumeAvailable'] != true
                  ? () => unawaited(_prepareRfiReply())
                  : null,
              onOpenDestination: _firstDestination() != null
                  ? () => unawaited(_openDestination())
                  : null,
            ),
          ),
        ),
      ],
    );
  }

  Widget _buildForm(BuildContext context) {
    final s = _s;
    final op = _operation;
    final reviewAvailable = op.schema.any((param) => param.name == 'dest');
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Expanded(
          child: SingleChildScrollView(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                _buildContentCard(context, s, op),
                if (_error.isNotEmpty) ...[
                  const SizedBox(height: 12),
                  Notice(kind: NoticeKind.error, child: Text(_error)),
                ],
              ],
            ),
          ),
        ),
        const SizedBox(height: 12),
        ActionBar(
          left: [
            if (reviewAvailable)
              DfButton(
                variant: DfButtonVariant.ghost,
                icon: Icon(AppIcons.folderOpen, size: remPx(context, 0.95)),
                onPressed: () => unawaited(_reviewFolder()),
                child: Text(s.t('action.review')),
              ),
          ],
          right: [
            DfButton(
              icon: Icon(AppIcons.eye, size: remPx(context, 0.95)),
              onPressed: () => unawaited(_preview()),
              child: Text(s.t('action.preview')),
            ),
          ],
        ),
      ],
    );
  }

  Widget _buildContentCard(BuildContext context, Strings s, OperationDef op) {
    final t = DocFlowTokens.of(context);
    final visible = [
      for (final param in op.schema)
        if (param.visibleWhen.entries.every(
          (entry) => _values[entry.key] == entry.value,
        ))
          param,
    ];
    final previous = _previousValues;
    return Container(
      padding: const EdgeInsets.all(16),
      decoration: BoxDecoration(
        color: t.surface,
        border: Border.all(color: t.line),
        borderRadius: BorderRadius.circular(t.radius),
        boxShadow: t.panelShadow,
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Icon(
                AppIcons.slidersHorizontal,
                size: remPx(context, 1.2),
                color: t.text,
              ),
              const SizedBox(width: 12),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      s.t('step.form'),
                      style: TextStyle(
                        fontSize: remPx(context, DocFlowTypeScale.h2Rem),
                        fontWeight: FontWeight.w700,
                        color: t.text,
                      ),
                    ),
                    const SizedBox(height: 3),
                    Text(
                      s.t(op.descriptionKey),
                      style: TextStyle(
                        fontSize: remPx(context, DocFlowTypeScale.captionRem),
                        height: 1.45,
                        color: t.muted,
                      ),
                    ),
                  ],
                ),
              ),
            ],
          ),
          const SizedBox(height: 16),
          if (op.supportsPresets) ...[
            PresetToolbar(
              bridge: _bridge,
              operation: op,
              values: _values,
              s: s,
              onApply: (preset) {
                _valuesRevision++;
                setState(() => _values = {..._values, ...preset.values});
                _scheduleSave();
              },
              onError: (message) => _showError(message),
            ),
            const SizedBox(height: 14),
          ],
          if (op.supportsPresets && previous != null) ...[
            _ResumeCard(
              title: s.t('params.previous.title'),
              help: s.t('params.previous.help'),
              actions: [
                DfButton(
                  variant: DfButtonVariant.secondary,
                  icon: Icon(AppIcons.rotateCcw, size: remPx(context, 0.95)),
                  onPressed: () {
                    _valuesRevision++;
                    setState(() => _values = {..._values, ...previous});
                    _scheduleSave();
                  },
                  child: Text(s.t('params.previous.apply')),
                ),
              ],
            ),
            const SizedBox(height: 14),
          ],
          for (final param in visible) ...[
            SchemaField(
              key: ValueKey(param.name),
              bridge: _bridge,
              operationId: op.id,
              param: param,
              value: _values[param.name],
              values: _values,
              s: s,
              onChange: (value) => _setValue(param.name, value),
              onError: _showError,
            ),
            const SizedBox(height: 14),
          ],
          if (op.id == 'files.batch_print') ...[
            _buildRfiFinder(context, s),
            const SizedBox(height: 14),
          ],
          if (_latestPrintQueue case final queue?) ...[
            _ResumeCard(
              title: s.t('print.resume.title'),
              help: s.t(
                'print.resume.summary',
                {'count': queue.resumable},
              ),
              actions: [
                DfButton(
                  variant: DfButtonVariant.ghost,
                  onPressed: () =>
                      unawaited(_discardPrintQueue(queue.queueId)),
                  child: Text(s.t('print.resume.discard')),
                ),
                DfButton(
                  variant: DfButtonVariant.secondary,
                  icon: Icon(AppIcons.rotateCcw, size: remPx(context, 0.95)),
                  onPressed: () =>
                      unawaited(_resumePrintQueue(queue.queueId)),
                  child: Text(s.t('print.resume.review')),
                ),
              ],
            ),
          ],
        ],
      ),
    );
  }

  Widget _buildRfiFinder(BuildContext context, Strings s) {
    final t = DocFlowTokens.of(context);
    final inputs = _rfiInputs;
    return Container(
      padding: const EdgeInsets.all(14),
      decoration: BoxDecoration(
        border: Border.all(color: t.line),
        borderRadius: BorderRadius.circular(t.radius),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            s.t('print.rfi.title'),
            style: TextStyle(
              fontSize: remPx(context, DocFlowTypeScale.bodyRem),
              fontWeight: FontWeight.w700,
              color: t.text,
            ),
          ),
          if (inputs.isNotEmpty) ...[
            const SizedBox(height: 10),
            Text(
              s.t('print.rfi.inputs'),
              style: TextStyle(
                fontSize: remPx(context, DocFlowTypeScale.captionRem),
                fontWeight: FontWeight.w600,
                color: t.muted,
              ),
            ),
            Wrap(
              spacing: 8,
              children: [
                DfButton(
                  variant: DfButtonVariant.ghost,
                  onPressed: () => setState(() {
                    _invalidateRfiFinder();
                    _excludedRfiInputs = const [];
                  }),
                  child: Text(s.t('action.selectAll')),
                ),
                DfButton(
                  variant: DfButtonVariant.ghost,
                  onPressed: () => setState(() {
                    _invalidateRfiFinder();
                    _excludedRfiInputs = inputs;
                  }),
                  child: Text(s.t('action.selectNone')),
                ),
              ],
            ),
            for (final input in inputs)
              _CheckRow(
                checked: !_excludedRfiInputs.contains(input),
                label: input,
                onChanged: (checked) => setState(() {
                  _invalidateRfiFinder();
                  _excludedRfiInputs = checked
                      ? [
                          for (final value in _excludedRfiInputs)
                            if (value != input) value,
                        ]
                      : [..._excludedRfiInputs, input];
                }),
              ),
          ],
          const SizedBox(height: 10),
          DfButton(
            variant: DfButtonVariant.secondary,
            onPressed: _rfiFinderBusy || inputs.isEmpty
                ? null
                : () => unawaited(_searchRfiDocuments()),
            child: Text(
              s.t(_rfiFinderBusy ? 'print.rfi.searching' : 'print.rfi.search'),
            ),
          ),
          if (_rfiFinderError.isNotEmpty) ...[
            const SizedBox(height: 10),
            Notice(kind: NoticeKind.error, child: Text(_rfiFinderError)),
          ],
          if (_rfiResolutions.isNotEmpty) ...[
            const SizedBox(height: 10),
            _buildRfiResults(context, s),
          ],
        ],
      ),
    );
  }

  Widget _buildRfiResults(BuildContext context, Strings s) {
    final t = DocFlowTokens.of(context);
    final textStyle = TextStyle(
      fontSize: remPx(context, DocFlowTypeScale.captionRem),
      height: 1.45,
      color: t.text,
    );
    final disciplines = <String>{
      for (final candidate in _rfiCandidates)
        if (_disciplinePattern.firstMatch(candidate.rfiCode)?.group(1)
            case final discipline?)
          discipline,
    };
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          s.t('print.rfi.found', {'count': _rfiCandidates.length}),
          style: textStyle,
        ),
        if (_rfiSearchTruncated) ...[
          const SizedBox(height: 8),
          Notice(kind: NoticeKind.warning, child: Text(s.t('print.rfi.truncated'))),
        ],
        for (final resolution in _rfiResolutions)
          _ResolutionBlock(
            resolution: resolution,
            textStyle: textStyle,
            s: s,
            onToggle: () {
              final group = [
                for (final candidate in _rfiCandidates)
                  if (candidate.requested == resolution.requested) candidate,
              ];
              final allSelected = group.every((candidate) => candidate.selected);
              _setCandidateGroup(
                (candidate) => candidate.requested == resolution.requested,
                !allSelected,
              );
            },
          ),
        if (_rfiFinderWasRun) ...[
          const SizedBox(height: 8),
          Wrap(
            spacing: 8,
            runSpacing: 8,
            children: [
              DfButton(
                variant: DfButtonVariant.ghost,
                onPressed: () => _setCandidateGroup((_) => true, true),
                child: Text(s.t('action.selectAll')),
              ),
              DfButton(
                variant: DfButtonVariant.ghost,
                onPressed: () => _setCandidateGroup((_) => true, false),
                child: Text(s.t('action.selectNone')),
              ),
              DfButton(
                variant: DfButtonVariant.ghost,
                onPressed: () => _setCandidateGroup(
                  (candidate) =>
                      candidate.documentType == RfiDocumentType.firstPage,
                  true,
                ),
                child: Text(s.t('print.rfi.selectTypeFirstPage')),
              ),
              DfButton(
                variant: DfButtonVariant.ghost,
                onPressed: () => _setCandidateGroup(
                  (candidate) =>
                      candidate.documentType == RfiDocumentType.sketch,
                  true,
                ),
                child: Text(s.t('print.rfi.selectTypeSketch')),
              ),
              DfButton(
                variant: DfButtonVariant.ghost,
                onPressed: () => _setCandidateGroup(
                  (candidate) =>
                      candidate.documentType == RfiDocumentType.firstPage,
                  false,
                ),
                child: Text(s.t('print.rfi.clearTypeFirstPage')),
              ),
              DfButton(
                variant: DfButtonVariant.ghost,
                onPressed: () => _setCandidateGroup(
                  (candidate) =>
                      candidate.documentType == RfiDocumentType.sketch,
                  false,
                ),
                child: Text(s.t('print.rfi.clearTypeSketch')),
              ),
              for (final discipline in disciplines)
                DfButton(
                  variant: DfButtonVariant.ghost,
                  onPressed: () => _setCandidateGroup(
                    (candidate) =>
                        candidate.rfiCode.contains('-RFI-$discipline-'),
                    true,
                  ),
                  child: Text(
                    s.t('print.rfi.selectDiscipline', {'discipline': discipline}),
                  ),
                ),
            ],
          ),
        ],
        for (final candidate in _rfiCandidates)
          _CandidateRow(
            candidate: candidate,
            typeLabel: s.t('print.rfi.type.${candidate.documentType.name}'),
            onChanged: (selected) => _setCandidateGroup(
              (row) =>
                  row.requested == candidate.requested &&
                  row.rfiCode == candidate.rfiCode &&
                  row.documentType == candidate.documentType &&
                  row.displayPath == candidate.displayPath,
              selected,
            ),
          ),
        if (_rfiCandidates.isEmpty) Text(s.t('print.rfi.noMatches'), style: textStyle),
      ],
    );
  }
}

/// A bordered block with a title row and action buttons (`.resume-queue-card`).
class _ResumeCard extends StatelessWidget {
  const _ResumeCard({
    required this.title,
    required this.help,
    required this.actions,
  });

  final String title;
  final String help;
  final List<Widget> actions;

  @override
  Widget build(BuildContext context) {
    final t = DocFlowTokens.of(context);
    return Container(
      padding: const EdgeInsets.all(14),
      decoration: BoxDecoration(
        color: t.surface2,
        border: Border.all(color: t.line),
        borderRadius: BorderRadius.circular(t.radius),
      ),
      child: Wrap(
        alignment: WrapAlignment.spaceBetween,
        crossAxisAlignment: WrapCrossAlignment.center,
        spacing: 12,
        runSpacing: 10,
        children: [
          ConstrainedBox(
            constraints: const BoxConstraints(maxWidth: 520),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  title,
                  style: TextStyle(
                    fontSize: remPx(context, DocFlowTypeScale.bodyRem),
                    fontWeight: FontWeight.w700,
                    color: t.text,
                  ),
                ),
                const SizedBox(height: 3),
                Text(
                  help,
                  style: TextStyle(
                    fontSize: remPx(context, DocFlowTypeScale.captionRem),
                    height: 1.45,
                    color: t.muted,
                  ),
                ),
              ],
            ),
          ),
          Wrap(spacing: 8, runSpacing: 8, children: actions),
        ],
      ),
    );
  }
}

/// A checkbox with its label, the whole row toggling (`.rfi-print-row`).
class _CheckRow extends StatelessWidget {
  const _CheckRow({
    required this.checked,
    required this.label,
    required this.onChanged,
    this.child,
  });

  final bool checked;
  final String label;
  final ValueChanged<bool> onChanged;
  final Widget? child;

  @override
  Widget build(BuildContext context) => InkWell(
    onTap: () => onChanged(!checked),
    child: Padding(
      padding: const EdgeInsets.symmetric(vertical: 4),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Checkbox(
            value: checked,
            onChanged: (value) => onChanged(value == true),
          ),
          Expanded(child: child ?? Text(label)),
        ],
      ),
    ),
  );
}

/// One requested RFI number and its resolution messages.
class _ResolutionBlock extends StatelessWidget {
  const _ResolutionBlock({
    required this.resolution,
    required this.textStyle,
    required this.s,
    required this.onToggle,
  });

  final RfiPrintResolution resolution;
  final TextStyle textStyle;
  final Strings s;
  final VoidCallback onToggle;

  @override
  Widget build(BuildContext context) {
    final codes = resolution.rfiCodes.isEmpty
        ? ''
        : ' — ${resolution.rfiCodes.join(', ')}';
    return Padding(
      padding: const EdgeInsets.only(bottom: 8),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            resolution.requested,
            style: textStyle.copyWith(fontWeight: FontWeight.w700),
          ),
          if (resolution.ambiguous)
            Text('${s.t('print.rfi.ambiguous')}$codes', style: textStyle),
          if (resolution.missingFirstPage)
            Text(s.t('print.rfi.missingFirstPage'), style: textStyle),
          if (resolution.missingSketch)
            Text(s.t('print.rfi.missingSketch'), style: textStyle),
          Align(
            alignment: AlignmentDirectional.centerStart,
            child: DfButton(
              variant: DfButtonVariant.ghost,
              onPressed: onToggle,
              child: Text(s.t('print.rfi.toggleRequest')),
            ),
          ),
        ],
      ),
    );
  }
}

/// One matched document with its selection checkbox (`.rfi-print-row`).
class _CandidateRow extends StatelessWidget {
  const _CandidateRow({
    required this.candidate,
    required this.typeLabel,
    required this.onChanged,
  });

  final RfiPrintCandidate candidate;
  final String typeLabel;
  final ValueChanged<bool> onChanged;

  @override
  Widget build(BuildContext context) {
    final t = DocFlowTokens.of(context);
    return Semantics(
      label: '${candidate.rfiCode} $typeLabel ${candidate.displayPath}',
      child: _CheckRow(
        checked: candidate.selected,
        label: candidate.rfiCode,
        onChanged: onChanged,
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              '${candidate.rfiCode} — $typeLabel',
              style: TextStyle(
                fontSize: remPx(context, DocFlowTypeScale.captionRem),
                fontWeight: FontWeight.w700,
                color: t.text,
              ),
            ),
            Text(
              candidate.displayPath,
              style: TextStyle(
                fontSize: remPx(context, 0.72),
                color: t.muted,
              ),
            ),
          ],
        ),
      ),
    );
  }
}
