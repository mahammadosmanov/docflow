// Modal, ported from `Modal` in ui.tsx and the `.modal-backdrop` / `.modal`
// rules. The widget fills its parent, so place it in a Stack above the screen.

import 'dart:math' as math;

import 'package:flutter/material.dart' hide IconButton, Switch;
import 'package:flutter/services.dart';

import '../../l10n/strings.dart';
import '../../theme/app_icons.dart';
import '../../theme/tokens.dart';
import 'buttons.dart';
import 'metrics.dart';

/// A dialog over a dimmed backdrop. Escape and a tap on the backdrop call
/// [onClose]. The close button's name comes from [s] ("action.close").
class Modal extends StatefulWidget {
  const Modal({
    super.key,
    required this.title,
    required this.onClose,
    required this.child,
    required this.s,
  });

  final String title;
  final VoidCallback onClose;
  final Widget child;
  final Strings s;

  @override
  State<Modal> createState() => _ModalState();
}

class _ModalState extends State<Modal> {
  @override
  void initState() {
    super.initState();
    // Like the React document capture listener, this sees Escape before any
    // focused child does, whichever child has focus.
    HardwareKeyboard.instance.addHandler(_onKey);
  }

  @override
  void dispose() {
    HardwareKeyboard.instance.removeHandler(_onKey);
    super.dispose();
  }

  bool _onKey(KeyEvent event) {
    if (event is! KeyDownEvent ||
        event.logicalKey != LogicalKeyboardKey.escape) {
      return false;
    }
    if (!mounted) return false;
    widget.onClose();
    return true;
  }

  @override
  Widget build(BuildContext context) {
    final t = DocFlowTokens.of(context);
    final viewportHeight = MediaQuery.sizeOf(context).height;
    final compact = viewportHeight <= 620;

    return LayoutBuilder(
      builder: (context, constraints) {
        final pad = compact ? 8.0 : 24.0;
        final boxWidth = math.min(460.0, constraints.maxWidth - 2 * pad);
        final boxMaxHeight = constraints.hasBoundedHeight
            ? constraints.maxHeight - 2 * pad
            : double.infinity;
        return Stack(
          fit: StackFit.expand,
          children: [
            Positioned.fill(
              child: GestureDetector(
                behavior: HitTestBehavior.opaque,
                onTap: widget.onClose,
                child: const ColoredBox(color: Color(0x94000000)),
              ),
            ),
            Align(
              alignment: compact ? Alignment.topCenter : Alignment.center,
              child: Padding(
                padding: EdgeInsets.all(pad),
                child: SizedBox(
                  width: math.max(0.0, boxWidth),
                  child: ConstrainedBox(
                    constraints: BoxConstraints(
                      maxHeight: math.max(0.0, boxMaxHeight),
                    ),
                    child: Semantics(
                      scopesRoute: true,
                      namesRoute: true,
                      explicitChildNodes: true,
                      label: widget.title,
                      child: DecoratedBox(
                        decoration: BoxDecoration(
                          color: t.surface,
                          border: Border.all(color: t.lineStrong),
                          borderRadius: BorderRadius.circular(9),
                          boxShadow: const [
                            BoxShadow(
                              color: Color(0x33000000),
                              offset: Offset(0, 16),
                              blurRadius: 48,
                            ),
                          ],
                        ),
                        child: Padding(
                          padding: const EdgeInsets.all(20),
                          child: Column(
                            mainAxisSize: MainAxisSize.min,
                            crossAxisAlignment: CrossAxisAlignment.stretch,
                            children: [
                              Padding(
                                padding: const EdgeInsets.only(bottom: 14),
                                child: Row(
                                  mainAxisAlignment:
                                      MainAxisAlignment.spaceBetween,
                                  children: [
                                    Flexible(
                                      child: Semantics(
                                        header: true,
                                        child: Text(
                                          widget.title,
                                          style: TextStyle(
                                            fontSize: remPx(context, 1.05),
                                            fontWeight: FontWeight.w700,
                                            letterSpacing:
                                                -0.015 * remPx(context, 1.05),
                                            color: t.text,
                                          ),
                                        ),
                                      ),
                                    ),
                                    IconButton(
                                      label: widget.s.t('action.close'),
                                      onPressed: widget.onClose,
                                      child: Icon(AppIcons.close),
                                    ),
                                  ],
                                ),
                              ),
                              Flexible(
                                child: SingleChildScrollView(
                                  child: widget.child,
                                ),
                              ),
                            ],
                          ),
                        ),
                      ),
                    ),
                  ),
                ),
              ),
            ),
          ],
        );
      },
    );
  }
}
