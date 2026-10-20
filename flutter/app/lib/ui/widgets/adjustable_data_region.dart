// AdjustableDataRegion and ColumnResizeHandle, ported from
// AdjustableDataRegion.tsx and AdjustableDataRegion.css.
//
// Both controls are separators with the same keyboard model: arrow keys step
// by 16px, Home goes to the minimum and End to the maximum. Each is announced
// as an adjustable slider with the value text the React version set.

import 'dart:math' as math;

import 'package:flutter/material.dart' hide IconButton, Switch;
import 'package:flutter/services.dart';

import '../../l10n/strings.dart';
import '../../theme/tokens.dart';
import 'metrics.dart';
import 'pressable.dart';

/// Smallest height the data region can be dragged to (`MIN_REGION_HEIGHT`).
const double minRegionHeight = 140;

/// Height a region starts at, and returns to with "Reset height".
const double defaultRegionHeight = 320;

/// Narrowest a column can be dragged to (`MIN_COLUMN_WIDTH`).
const double minColumnWidth = 48;

/// Widest a column can be dragged to (`MAX_COLUMN_WIDTH`).
const double maxColumnWidth = 720;

/// One arrow-key step, for both the region and the column (`COLUMN_STEP`).
const double columnStep = 16;

/// Height assumed for the controls row before it has been laid out once. The
/// CSS sets a 28px minimum including its 3px top padding.
const double _controlsFallbackHeight = 28;

/// Clamps a column width to [minWidth]..[maxColumnWidth].
double clampColumnWidth(double width, {double minWidth = minColumnWidth}) =>
    math.min(maxColumnWidth, math.max(minWidth, width));

/// JavaScript's Math.round: halves round up.
double _jsRound(double value) => (value + 0.5).floorToDouble();

enum _Adjust { decrease, increase, min, max }

class _AdjustIntent extends Intent {
  const _AdjustIntent(this.kind);

  final _Adjust kind;
}

/// The keyboard, focus and assistive-technology behaviour shared by both
/// separators. [builder] receives the hover and focus state.
class _Adjustable extends StatefulWidget {
  const _Adjustable({
    required this.label,
    required this.value,
    required this.increasedValue,
    required this.decreasedValue,
    required this.vertical,
    required this.onAdjust,
    required this.ringRadius,
    required this.builder,
    this.focusNode,
    this.mouseCursor = SystemMouseCursors.basic,
  });

  final String label;
  final String value;

  /// The value after one step up and down. Semantics needs them alongside
  /// [value] for the adjust actions.
  final String increasedValue;
  final String decreasedValue;

  /// True for a horizontal separator (arrow up and down), false for a
  /// vertical one (arrow left and right).
  final bool vertical;
  final ValueChanged<_Adjust> onAdjust;
  final double ringRadius;
  final FocusNode? focusNode;
  final MouseCursor mouseCursor;
  final Widget Function(BuildContext context, bool hovered, bool focused)
  builder;

  @override
  State<_Adjustable> createState() => _AdjustableState();
}

class _AdjustableState extends State<_Adjustable> {
  bool _hovered = false;
  bool _focused = false;

  @override
  Widget build(BuildContext context) {
    final axis = widget.vertical
        ? <ShortcutActivator, Intent>{
            const SingleActivator(LogicalKeyboardKey.arrowUp):
                const _AdjustIntent(_Adjust.decrease),
            const SingleActivator(LogicalKeyboardKey.arrowDown):
                const _AdjustIntent(_Adjust.increase),
          }
        : <ShortcutActivator, Intent>{
            const SingleActivator(LogicalKeyboardKey.arrowLeft):
                const _AdjustIntent(_Adjust.decrease),
            const SingleActivator(LogicalKeyboardKey.arrowRight):
                const _AdjustIntent(_Adjust.increase),
          };

    return Semantics(
      slider: true,
      label: widget.label,
      value: widget.value,
      increasedValue: widget.increasedValue,
      decreasedValue: widget.decreasedValue,
      onIncrease: () => widget.onAdjust(_Adjust.increase),
      onDecrease: () => widget.onAdjust(_Adjust.decrease),
      child: FocusableActionDetector(
        focusNode: widget.focusNode,
        mouseCursor: widget.mouseCursor,
        onShowHoverHighlight: (value) => setState(() => _hovered = value),
        onShowFocusHighlight: (value) => setState(() => _focused = value),
        shortcuts: {
          ...axis,
          const SingleActivator(LogicalKeyboardKey.home): const _AdjustIntent(
            _Adjust.min,
          ),
          const SingleActivator(LogicalKeyboardKey.end): const _AdjustIntent(
            _Adjust.max,
          ),
        },
        actions: <Type, Action<Intent>>{
          _AdjustIntent: CallbackAction<_AdjustIntent>(
            onInvoke: (intent) {
              widget.onAdjust(intent.kind);
              return null;
            },
          ),
        },
        child: FocusRing(
          visible: _focused,
          radius: widget.ringRadius,
          child: widget.builder(context, _hovered, _focused),
        ),
      ),
    );
  }
}

/// A data block with a resizable height. The child gets a tight height and
/// should fill it, scrolling internally if needed. Controls under the block
/// reset the height, fit it to the viewport, and drag or key-resize it.
class AdjustableDataRegion extends StatefulWidget {
  const AdjustableDataRegion({
    super.key,
    required this.child,
    required this.s,
    required this.label,
    this.focusNode,
  });

  final Widget child;
  final Strings s;

  /// Names the region and its handle ("Resize {label} height").
  final String label;

  /// Optional focus node for the resize handle.
  final FocusNode? focusNode;

  @override
  State<AdjustableDataRegion> createState() => _AdjustableDataRegionState();
}

class _AdjustableDataRegionState extends State<AdjustableDataRegion> {
  double _height = defaultRegionHeight;
  double _maxHeight = defaultRegionHeight;
  double? _dragOrigin;
  double _dragDistance = 0;

  final GlobalKey _regionKey = GlobalKey();
  final GlobalKey _controlsKey = GlobalKey();
  final GlobalKey _handleKey = GlobalKey();

  double _clamp(double height) =>
      math.min(math.max(height, minRegionHeight), _maxHeight);

  double _controlsHeight() {
    final box = _controlsKey.currentContext?.findRenderObject();
    return box is RenderBox && box.hasSize
        ? box.size.height
        : _controlsFallbackHeight;
  }

  void _afterFrame(VoidCallback action) {
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) action();
    });
  }

  /// Scrolls the nearest scrollable so the region's top meets its top edge.
  void _revealRegion() {
    final context = _regionKey.currentContext;
    if (context != null) {
      Scrollable.ensureVisible(context, alignment: 0, duration: Duration.zero);
    }
  }

  /// Keeps the handle visible after a keyboard resize.
  void _revealHandle() {
    final context = _handleKey.currentContext;
    if (context != null) {
      Scrollable.ensureVisible(
        context,
        alignment: 1,
        alignmentPolicy: ScrollPositionAlignmentPolicy.keepVisibleAtEnd,
        duration: Duration.zero,
      );
    }
  }

  void _reset() => setState(() => _height = _clamp(defaultRegionHeight));

  void _fit() {
    setState(() => _height = _maxHeight);
    _afterFrame(_revealRegion);
  }

  void _adjust(_Adjust kind) {
    setState(() {
      _height = switch (kind) {
        _Adjust.decrease => _clamp(_height - columnStep),
        _Adjust.increase => _clamp(_height + columnStep),
        _Adjust.min => _clamp(minRegionHeight),
        _Adjust.max => _maxHeight,
      };
    });
    _afterFrame(_revealHandle);
  }

  void _dragStart(DragStartDetails details) {
    _dragOrigin = _height;
    _dragDistance = 0;
  }

  void _dragUpdate(DragUpdateDetails details) {
    final origin = _dragOrigin;
    if (origin == null) return;
    _dragDistance += details.primaryDelta ?? 0;
    setState(() => _height = _clamp(origin + _dragDistance));
  }

  void _dragEnd() => _dragOrigin = null;

  @override
  Widget build(BuildContext context) {
    final t = DocFlowTokens.of(context);
    return LayoutBuilder(
      builder: (context, constraints) {
        final available = constraints.hasBoundedHeight
            ? constraints.maxHeight
            : MediaQuery.sizeOf(context).height;
        _maxHeight = math.max(
          minRegionHeight,
          (available - _controlsHeight()).floorToDouble(),
        );
        final height = _clamp(_height);
        if (height != _height) {
          // The bounds shrank below the stored height: store the clamped value,
          // as the React version did in its resize observer.
          _afterFrame(() => setState(() => _height = _clamp(_height)));
        }

        return Semantics(
          container: true,
          explicitChildNodes: true,
          label: widget.label,
          child: Column(
            key: _regionKey,
            crossAxisAlignment: CrossAxisAlignment.stretch,
            mainAxisSize: MainAxisSize.min,
            children: [
              SizedBox(
                height: height,
                child: ClipRect(child: widget.child),
              ),
              Padding(
                key: _controlsKey,
                padding: const EdgeInsets.only(top: 3),
                child: ConstrainedBox(
                  constraints: const BoxConstraints(minHeight: 25),
                  child: Row(
                    mainAxisAlignment: MainAxisAlignment.end,
                    children: [
                      _ControlButton(
                        label: widget.s.t('layout.data.resetHeight'),
                        onPressed: _reset,
                      ),
                      const SizedBox(width: 6),
                      _ControlButton(
                        label: widget.s.t('layout.data.fit'),
                        onPressed: _fit,
                      ),
                      // The CSS gap of 6px plus the handle's 3px left margin.
                      const SizedBox(width: 9),
                      KeyedSubtree(
                        key: _handleKey,
                        child: _Adjustable(
                          label: widget.s.t('layout.data.resizeHeight', {
                            'label': widget.label,
                          }),
                          value: '${_height.round()}',
                          increasedValue:
                              '${_clamp(_height + columnStep).round()}',
                          decreasedValue:
                              '${_clamp(_height - columnStep).round()}',
                          vertical: true,
                          ringRadius: 10,
                          mouseCursor: SystemMouseCursors.resizeRow,
                          focusNode: widget.focusNode,
                          onAdjust: _adjust,
                          builder: (context, hovered, focused) {
                            return GestureDetector(
                              behavior: HitTestBehavior.opaque,
                              onVerticalDragStart: _dragStart,
                              onVerticalDragUpdate: _dragUpdate,
                              onVerticalDragEnd: (_) => _dragEnd(),
                              onVerticalDragCancel: _dragEnd,
                              child: SizedBox(
                                width: 34,
                                height: 14,
                                child: DecoratedBox(
                                  decoration: BoxDecoration(
                                    border: Border.all(color: t.line),
                                    borderRadius: BorderRadius.circular(10),
                                  ),
                                  child: CustomPaint(
                                    painter: _GripPainter(t.text),
                                  ),
                                ),
                              ),
                            );
                          },
                        ),
                      ),
                    ],
                  ),
                ),
              ),
            ],
          ),
        );
      },
    );
  }
}

/// The two short bars drawn inside the region handle (the CSS `::after`).
class _GripPainter extends CustomPainter {
  _GripPainter(this.color);

  final Color color;

  @override
  void paint(Canvas canvas, Size size) {
    final paint = Paint()..color = color;
    canvas.drawRect(const Rect.fromLTWH(11, 6, 12, 2), paint);
    canvas.drawRect(const Rect.fromLTWH(11, 9, 12, 2), paint);
  }

  @override
  bool shouldRepaint(_GripPainter old) => old.color != color;
}

class _ControlButton extends StatelessWidget {
  const _ControlButton({required this.label, required this.onPressed});

  final String label;
  final VoidCallback onPressed;

  @override
  Widget build(BuildContext context) {
    final t = DocFlowTokens.of(context);
    return Pressable(
      label: label,
      onPressed: onPressed,
      builder: (context, hovered, focused) {
        return FocusRing(
          visible: focused,
          radius: 4,
          child: Container(
            padding: const EdgeInsets.symmetric(horizontal: 7, vertical: 3),
            decoration: BoxDecoration(
              color: hovered ? t.surface3 : Colors.transparent,
              border: Border.all(color: t.line),
              borderRadius: BorderRadius.circular(4),
            ),
            child: ExcludeSemantics(
              child: Text(
                label,
                style: TextStyle(
                  fontSize: remPx(context, 0.75),
                  color: hovered ? t.text : t.muted,
                ),
              ),
            ),
          ),
        );
      },
    );
  }
}

/// A vertical separator for resizing a table column. It sits at the right edge
/// of a header cell, so place it with `Positioned(top: 0, bottom: 0, right: 0)`
/// in a Stack.
///
/// Dragging calls [onWidthPreview] on every change and then [onWidthChange] on
/// release. A cancelled drag calls [onWidthPreview] with the starting width.
class ColumnResizeHandle extends StatefulWidget {
  const ColumnResizeHandle({
    super.key,
    required this.width,
    required this.label,
    required this.s,
    required this.onWidthChange,
    this.onWidthPreview,
    this.minWidth = minColumnWidth,
    this.focusNode,
  });

  final double width;

  /// The column's name ("Resize {column} column").
  final String label;
  final Strings s;
  final ValueChanged<double> onWidthChange;
  final ValueChanged<double>? onWidthPreview;
  final double minWidth;
  final FocusNode? focusNode;

  @override
  State<ColumnResizeHandle> createState() => _ColumnResizeHandleState();
}

class _ColumnResizeHandleState extends State<ColumnResizeHandle> {
  double? _dragOrigin;
  double _dragDistance = 0;
  double _lastWidth = 0;

  void _dragStart(DragStartDetails details) {
    _dragOrigin = widget.width;
    _dragDistance = 0;
    _lastWidth = _jsRound(widget.width);
  }

  void _dragUpdate(DragUpdateDetails details) {
    final origin = _dragOrigin;
    if (origin == null) return;
    _dragDistance += details.primaryDelta ?? 0;
    final next = _jsRound(
      clampColumnWidth(origin + _dragDistance, minWidth: widget.minWidth),
    );
    if (next == _lastWidth) return;
    setState(() => _lastWidth = next);
    widget.onWidthPreview?.call(next);
  }

  void _dragEnd() {
    if (_dragOrigin == null) return;
    _dragOrigin = null;
    widget.onWidthChange(_lastWidth);
  }

  void _dragCancel() {
    final origin = _dragOrigin;
    if (origin == null) return;
    _dragOrigin = null;
    final restored = _jsRound(
      clampColumnWidth(origin, minWidth: widget.minWidth),
    );
    setState(() => _lastWidth = restored);
    widget.onWidthPreview?.call(restored);
  }

  void _adjust(_Adjust kind) {
    final next = switch (kind) {
      _Adjust.decrease => clampColumnWidth(
        widget.width - columnStep,
        minWidth: widget.minWidth,
      ),
      _Adjust.increase => clampColumnWidth(
        widget.width + columnStep,
        minWidth: widget.minWidth,
      ),
      _Adjust.min => widget.minWidth,
      _Adjust.max => maxColumnWidth,
    };
    widget.onWidthChange(next);
  }

  @override
  Widget build(BuildContext context) {
    final t = DocFlowTokens.of(context);
    final dragging = _dragOrigin != null;
    final shown = dragging ? _lastWidth : _jsRound(widget.width);
    return SizedBox(
      width: 14,
      child: _Adjustable(
        label: widget.s.t('layout.column.resize', {'column': widget.label}),
        value: widget.s.t('layout.column.width', {'width': shown.round()}),
        increasedValue: widget.s.t('layout.column.width', {
          'width': clampColumnWidth(
            widget.width + columnStep,
            minWidth: widget.minWidth,
          ).round(),
        }),
        decreasedValue: widget.s.t('layout.column.width', {
          'width': clampColumnWidth(
            widget.width - columnStep,
            minWidth: widget.minWidth,
          ).round(),
        }),
        vertical: false,
        ringRadius: 0,
        mouseCursor: SystemMouseCursors.resizeColumn,
        focusNode: widget.focusNode,
        onAdjust: _adjust,
        builder: (context, hovered, focused) {
          final active = hovered || focused;
          return GestureDetector(
            behavior: HitTestBehavior.opaque,
            onHorizontalDragStart: _dragStart,
            onHorizontalDragUpdate: _dragUpdate,
            onHorizontalDragEnd: (_) => _dragEnd(),
            onHorizontalDragCancel: _dragCancel,
            child: Stack(
              fit: StackFit.expand,
              children: [
                Positioned(
                  left: 4,
                  top: 0,
                  bottom: 0,
                  width: 2,
                  child: FractionallySizedBox(
                    heightFactor: 0.5,
                    alignment: Alignment.center,
                    child: ColoredBox(color: active ? t.text : t.line),
                  ),
                ),
              ],
            ),
          );
        },
      ),
    );
  }
}
