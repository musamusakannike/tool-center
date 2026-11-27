import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import 'package:material_ui/material_ui.dart';
import 'package:path/path.dart' as p;

import '../../services/core_service.dart';
import '../theme/app_theme.dart';

/// Parses `reportJson` from AI tools (`ai.remove_bg`, `ai.upscale`, `ai.ocr`) and renders rich
/// visual result controls (Before/After split slider with checkerboard transparency, or OCR viewer).
class AiJobResultEnhancement extends StatelessWidget {
  final String rawReportJson;
  final ToolCenterBackend backend;

  const AiJobResultEnhancement({
    super.key,
    required this.rawReportJson,
    required this.backend,
  });

  static bool isAiVisualReport(String? rawJson) {
    if (rawJson == null || rawJson.isEmpty) return false;
    try {
      final map = jsonDecode(rawJson);
      if (map is Map<String, dynamic>) {
        final kind = map['tool_kind']?.toString() ?? '';
        return kind == 'ai.remove_bg' ||
            kind == 'ai.upscale' ||
            kind == 'ai.ocr';
      }
    } catch (_) {}
    return false;
  }

  @override
  Widget build(BuildContext context) {
    Map<String, dynamic>? parsed;
    try {
      final decoded = jsonDecode(rawReportJson);
      if (decoded is Map<String, dynamic>) {
        parsed = decoded;
      }
    } catch (_) {}

    if (parsed == null) {
      return const SizedBox.shrink();
    }

    final toolKind = parsed['tool_kind']?.toString() ?? '';
    if (toolKind == 'ai.remove_bg' || toolKind == 'ai.upscale') {
      return BeforeAfterComparisonCard(report: parsed, backend: backend);
    }

    return const SizedBox.shrink();
  }
}

/// Interactive Before / After comparison slider with checkerboard transparency canvas and batch switcher.
class BeforeAfterComparisonCard extends StatefulWidget {
  final Map<String, dynamic> report;
  final ToolCenterBackend backend;

  const BeforeAfterComparisonCard({
    super.key,
    required this.report,
    required this.backend,
  });

  @override
  State<BeforeAfterComparisonCard> createState() =>
      _BeforeAfterComparisonCardState();
}

class _BeforeAfterComparisonCardState extends State<BeforeAfterComparisonCard> {
  int _selectedIndex = 0;
  double _splitFraction = 0.5;
  Uint8List? _beforeBytes;
  Uint8List? _afterBytes;
  bool _loading = false;
  String? _loadError;

  List<Map<String, dynamic>> get _items {
    final raw = widget.report['items'];
    if (raw is List) {
      return raw.whereType<Map>().map(Map<String, dynamic>.from).toList();
    }
    return const [];
  }

  @override
  void initState() {
    super.initState();
    _loadSelectedPair();
  }

  @override
  void didUpdateWidget(covariant BeforeAfterComparisonCard oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.report != widget.report) {
      _selectedIndex = 0;
      _loadSelectedPair();
    }
  }

  Future<Uint8List> _readPreviewOrDisk(String path) async {
    try {
      final decoded = await widget.backend.renderDecodedImagePreview(
        path,
        maxDimension: 1600,
      );
      if (decoded.isNotEmpty) return decoded;
    } catch (_) {}
    final f = File(path);
    if (await f.exists()) {
      return f.readAsBytes();
    }
    throw StateError('Image file unavailable: $path');
  }

  Future<void> _loadSelectedPair() async {
    final items = _items;
    if (items.isEmpty) return;
    final idx = _selectedIndex.clamp(0, items.length - 1);
    final item = items[idx];
    final inPath = item['input_path']?.toString() ?? '';
    final outPath = item['output_path']?.toString() ?? '';
    if (inPath.isEmpty || outPath.isEmpty) return;

    setState(() {
      _loading = true;
      _loadError = null;
    });

    try {
      final results = await Future.wait([
        _readPreviewOrDisk(inPath),
        _readPreviewOrDisk(outPath),
      ]);
      if (!mounted) return;
      setState(() {
        _beforeBytes = results[0];
        _afterBytes = results[1];
        _loading = false;
      });
    } catch (e) {
      if (!mounted) return;
      setState(() {
        _loadError = e.toString();
        _loading = false;
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    final items = _items;
    if (items.isEmpty) return const SizedBox.shrink();

    final theme = Theme.of(context);
    final scheme = theme.colorScheme;
    final safeIdx = _selectedIndex.clamp(0, items.length - 1);
    final current = items[safeIdx];
    final provider =
        (widget.report['active_provider']?.toString() ?? 'cpu').toUpperCase();
    final modelName = widget.report['model_name']?.toString() ?? 'AI Model';
    final outW = current['output_width'] ?? current['width'];
    final outH = current['output_height'] ?? current['height'];
    final inW = current['input_width'];
    final inH = current['input_height'];

    String dimText = '';
    if (inW != null && inH != null && outW != null && outH != null) {
      dimText = '$inW×$inH → $outW×$outH px';
    } else if (outW != null && outH != null) {
      dimText = '$outW×$outH px';
    }

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Row(
          children: [
            Icon(Icons.compare_rounded, size: 18, color: scheme.primary),
            const SizedBox(width: 8),
            Text(
              'Before / After Comparison',
              style: theme.textTheme.titleSmall?.copyWith(
                fontWeight: FontWeight.w700,
              ),
            ),
            const Spacer(),
            if (dimText.isNotEmpty) ...[
              Text(
                dimText,
                style: theme.textTheme.bodySmall?.copyWith(
                  color: scheme.onSurfaceVariant,
                  fontWeight: FontWeight.w600,
                ),
              ),
              const SizedBox(width: 10),
            ],
            Container(
              padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 3),
              decoration: BoxDecoration(
                color: scheme.secondaryContainer,
                borderRadius: BorderRadius.circular(999),
              ),
              child: Text(
                '$modelName • $provider',
                style: theme.textTheme.labelSmall?.copyWith(
                  color: scheme.onSecondaryContainer,
                  fontWeight: FontWeight.w600,
                ),
              ),
            ),
          ],
        ),
        if (items.length > 1) ...[
          const SizedBox(height: 10),
          SingleChildScrollView(
            scrollDirection: Axis.horizontal,
            child: Row(
              children: [
                for (int i = 0; i < items.length; i++) ...[
                  ChoiceChip(
                    label: Text(
                      p.basename(items[i]['output_path']?.toString() ?? '#${i + 1}'),
                    ),
                    selected: i == safeIdx,
                    onSelected: (_) {
                      setState(() => _selectedIndex = i);
                      _loadSelectedPair();
                    },
                  ),
                  if (i < items.length - 1) const SizedBox(width: 8),
                ],
              ],
            ),
          ),
        ],
        const SizedBox(height: 10),
        ClipRRect(
          borderRadius: BorderRadius.circular(AppTheme.radiusSm),
          child: Container(
            height: 360,
            decoration: BoxDecoration(
              border: Border.all(color: scheme.outlineVariant),
              borderRadius: BorderRadius.circular(AppTheme.radiusSm),
            ),
            child: _loading
                ? const Center(child: CircularProgressIndicator())
                : _loadError != null
                    ? Center(
                        child: Padding(
                          padding: const EdgeInsets.all(16),
                          child: Text(
                            _loadError!,
                            style: TextStyle(color: scheme.error),
                          ),
                        ),
                      )
                    : (_beforeBytes != null && _afterBytes != null)
                        ? LayoutBuilder(
                            builder: (context, constraints) {
                              final maxW = constraints.maxWidth;
                              final splitX = maxW * _splitFraction;
                              return GestureDetector(
                                behavior: HitTestBehavior.opaque,
                                onHorizontalDragUpdate: (details) {
                                  setState(() {
                                    _splitFraction = (details.localPosition.dx /
                                            maxW)
                                        .clamp(0.0, 1.0);
                                  });
                                },
                                onTapDown: (details) {
                                  setState(() {
                                    _splitFraction = (details.localPosition.dx /
                                            maxW)
                                        .clamp(0.0, 1.0);
                                  });
                                },
                                child: Stack(
                                  fit: StackFit.expand,
                                  children: [
                                    // Checkerboard transparency backdrop
                                    CheckerboardCanvas(
                                      isDark:
                                          theme.brightness == Brightness.dark,
                                    ),
                                    // Full "AFTER" layer
                                    Image.memory(
                                      _afterBytes!,
                                      fit: BoxFit.contain,
                                      gaplessPlayback: true,
                                    ),
                                    // Clipped "BEFORE" layer on the left side
                                    ClipRect(
                                      clipper: _LeftSplitClipper(
                                        fraction: _splitFraction,
                                      ),
                                      child: Container(
                                        color: scheme.surfaceContainerLowest,
                                        child: Image.memory(
                                          _beforeBytes!,
                                          fit: BoxFit.contain,
                                          gaplessPlayback: true,
                                        ),
                                      ),
                                    ),
                                    // Vertical split line + drag thumb
                                    Positioned(
                                      left: (splitX - 1).clamp(0.0, maxW - 2),
                                      top: 0,
                                      bottom: 0,
                                      child: Container(
                                        width: 2,
                                        color: Colors.white,
                                      ),
                                    ),
                                    Positioned(
                                      left: (splitX - 18).clamp(0.0, maxW - 36),
                                      top: 162,
                                      child: DecoratedBox(
                                        decoration: BoxDecoration(
                                          color: scheme.primary,
                                          shape: BoxShape.circle,
                                          boxShadow: const [
                                            BoxShadow(
                                              blurRadius: 6,
                                              color: Colors.black38,
                                            ),
                                          ],
                                        ),
                                        child: SizedBox(
                                          width: 36,
                                          height: 36,
                                          child: Icon(
                                            Icons.swap_horiz_rounded,
                                            size: 20,
                                            color: scheme.onPrimary,
                                          ),
                                        ),
                                      ),
                                    ),
                                    // BEFORE / AFTER corner badges
                                    Positioned(
                                      left: 10,
                                      top: 10,
                                      child: _CornerBadge(label: 'BEFORE'),
                                    ),
                                    Positioned(
                                      right: 10,
                                      top: 10,
                                      child: _CornerBadge(label: 'AFTER'),
                                    ),
                                  ],
                                ),
                              );
                            },
                          )
                        : const SizedBox.shrink(),
          ),
        ),
        Row(
          children: [
            Text('Before', style: theme.textTheme.labelSmall),
            Expanded(
              child: Slider(
                value: _splitFraction,
                onChanged: (v) => setState(() => _splitFraction = v),
              ),
            ),
            Text('After', style: theme.textTheme.labelSmall),
          ],
        ),
      ],
    );
  }
}

class _CornerBadge extends StatelessWidget {
  final String label;

  const _CornerBadge({required this.label});

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 4),
      decoration: BoxDecoration(
        color: Colors.black.withValues(alpha: 0.65),
        borderRadius: BorderRadius.circular(6),
      ),
      child: Text(
        label,
        style: const TextStyle(
          color: Colors.white,
          fontSize: 11,
          fontWeight: FontWeight.w700,
          letterSpacing: 0.6,
        ),
      ),
    );
  }
}

class _LeftSplitClipper extends CustomClipper<Rect> {
  final double fraction;

  const _LeftSplitClipper({required this.fraction});

  @override
  Rect getClip(Size size) {
    return Rect.fromLTWH(0, 0, size.width * fraction.clamp(0.0, 1.0), size.height);
  }

  @override
  bool shouldReclip(covariant _LeftSplitClipper oldClipper) {
    return oldClipper.fraction != fraction;
  }
}

/// Crisp transparency checkerboard canvas for previewing RGBA cutouts.
class CheckerboardCanvas extends StatelessWidget {
  final bool isDark;
  final double cellSize;

  const CheckerboardCanvas({
    super.key,
    required this.isDark,
    this.cellSize = 14.0,
  });

  @override
  Widget build(BuildContext context) {
    return CustomPaint(
      painter: _CheckerboardPainter(isDark: isDark, cellSize: cellSize),
    );
  }
}

class _CheckerboardPainter extends CustomPainter {
  final bool isDark;
  final double cellSize;

  const _CheckerboardPainter({required this.isDark, required this.cellSize});

  @override
  void paint(Canvas canvas, Size size) {
    final baseColor =
        isDark ? const Color(0xFF1E222A) : const Color(0xFFF3F4F6);
    final altColor =
        isDark ? const Color(0xFF2A303C) : const Color(0xFFE2E5EC);

    canvas.drawRect(
      Offset.zero & size,
      Paint()..color = baseColor,
    );

    final altPaint = Paint()..color = altColor;
    final cols = (size.width / cellSize).ceil();
    final rows = (size.height / cellSize).ceil();

    for (int r = 0; r < rows; r++) {
      for (int c = (r % 2 == 0 ? 1 : 0); c < cols; c += 2) {
        final left = c * cellSize;
        final top = r * cellSize;
        final w = (left + cellSize <= size.width) ? cellSize : size.width - left;
        final h = (top + cellSize <= size.height) ? cellSize : size.height - top;
        canvas.drawRect(Rect.fromLTWH(left, top, w, h), altPaint);
      }
    }
  }

  @override
  bool shouldRepaint(covariant _CheckerboardPainter oldDelegate) {
    return oldDelegate.isDark != isDark || oldDelegate.cellSize != cellSize;
  }
}
