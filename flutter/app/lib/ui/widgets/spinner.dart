// The `.spin` loader: LoaderCircle turning at 900ms per turn. It holds still
// when the platform asks for reduced motion (`prefers-reduced-motion`).

import 'package:flutter/material.dart' hide IconButton, Switch;

import '../../theme/app_icons.dart';

class Spinner extends StatefulWidget {
  const Spinner({super.key, required this.size, required this.color});

  final double size;
  final Color color;

  @override
  State<Spinner> createState() => _SpinnerState();
}

class _SpinnerState extends State<Spinner> with SingleTickerProviderStateMixin {
  late final AnimationController _controller = AnimationController(
    vsync: this,
    duration: const Duration(milliseconds: 900),
  );
  bool? _reduceMotion;

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    final reduce = MediaQuery.disableAnimationsOf(context);
    if (reduce == _reduceMotion) return;
    _reduceMotion = reduce;
    if (reduce) {
      _controller.stop();
      _controller.value = 0;
    } else {
      _controller.repeat();
    }
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return RotationTransition(
      turns: _controller,
      child: Icon(
        AppIcons.loaderCircle,
        size: widget.size,
        color: widget.color,
      ),
    );
  }
}
