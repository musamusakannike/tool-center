import 'dart:async';
import 'dart:io';
import 'dart:typed_data';

import 'package:desktop_drop/desktop_drop.dart';
import 'package:flutter_svg/flutter_svg.dart';
import 'package:material_ui/material_ui.dart';
import 'package:media_kit/media_kit.dart';
import 'package:media_kit_video/media_kit_video.dart';

import '../../../l10n/app_localizations.dart';
import '../../models/preview_models.dart';
import '../../services/core_service.dart';
import '../../state/app_controller.dart';
import '../theme/app_theme.dart';
import 'document_preview_widgets.dart';

class FilePreviewView extends StatefulWidget {
  final AppController controller;
  final String filePath;

  const FilePreviewView({
    super.key,
    required this.controller,
    required this.filePath,
  });

  @override
  State<FilePreviewView> createState() => _FilePreviewViewState();
}

class _FilePreviewViewState extends State<FilePreviewView> {
  Future<FileInspectionModel>? _inspectionFuture;

  @override
  void initState() {
    super.initState();
    _loadInspection();
  }

  @override
  void didUpdateWidget(covariant FilePreviewView oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.filePath != widget.filePath) {
      _loadInspection();
    }
  }

  void _loadInspection() {
    _inspectionFuture = widget.controller.backend.inspectFile(widget.filePath);
  }

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context);
    final theme = Theme.of(context);

    return DropTarget(
      onDragDone: (details) {
        final next = details.files.firstOrNull?.path;
        if (next != null && next.isNotEmpty) {
          widget.controller.openPreview(next);
        }
      },
      child: FutureBuilder<FileInspectionModel>(
        future: _inspectionFuture,
        builder: (context, snapshot) {
          if (snapshot.connectionState == ConnectionState.waiting) {
            return const Center(child: CircularProgressIndicator());
          }
          if (snapshot.hasError) {
            return _PreviewErrorState(
              message: snapshot.error.toString(),
              onBack: widget.controller.goHome,
            );
          }
          final info = snapshot.data!;
          return Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              // Top Preview Toolbar
              Container(
                padding: const EdgeInsets.symmetric(
                  horizontal: 20,
                  vertical: 12,
                ),
                decoration: BoxDecoration(
                  color: theme.cardTheme.color,
                  border: Border(bottom: BorderSide(color: theme.dividerColor)),
                ),
                child: Row(
                  children: [
                    OutlinedButton.icon(
                      onPressed: widget.controller.goHome,
                      icon: const Icon(Icons.arrow_back_rounded, size: 18),
                      label: Text(l10n.backToHomeButton),
                    ),
                    const SizedBox(width: 16),
                    Expanded(
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Text(
                            info.fileName,
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                            style: theme.textTheme.titleMedium?.copyWith(
                              fontWeight: FontWeight.w700,
                            ),
                          ),
                          Text(
                            info.path,
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                            style: theme.textTheme.bodySmall?.copyWith(
                              color: theme.colorScheme.onSurfaceVariant,
                            ),
                          ),
                        ],
                      ),
                    ),
                    const SizedBox(width: 12),
                    OutlinedButton.icon(
                      onPressed: () => openInSystemApp(info.path),
                      icon: const Icon(Icons.open_in_new_rounded, size: 16),
                      label: Text(l10n.openInSystemAppButton),
                    ),
                    const SizedBox(width: 8),
                    OutlinedButton.icon(
                      onPressed: () => revealInSystemFolder(info.path),
                      icon: const Icon(Icons.folder_open_rounded, size: 16),
                      label: Text(l10n.revealInFolderButton),
                    ),
                  ],
                ),
              ),

              // Main Split Area: Left = Content Preview, Right = Metadata & Compatible Tools Panel
              Expanded(
                child: Row(
                  children: [
                    Expanded(
                      child: Padding(
                        padding: const EdgeInsets.all(16),
                        child: Card(
                          clipBehavior: Clip.antiAlias,
                          child: _buildPreviewContent(context, info),
                        ),
                      ),
                    ),
                    _MetadataSidebar(
                      info: info,
                      controller: widget.controller,
                    ),
                  ],
                ),
              ),
            ],
          );
        },
      ),
    );
  }

  Widget _buildPreviewContent(BuildContext context, FileInspectionModel info) {
    final l10n = AppLocalizations.of(context);

    switch (info.previewKind) {
      case 'image_native':
        return InteractiveViewer(
          minScale: 0.2,
          maxScale: 8.0,
          child: Center(
            child: Image.file(
              File(info.path),
              fit: BoxFit.contain,
              errorBuilder: (ctx, err, _) => Center(
                child: Text('Unable to render image: $err'),
              ),
            ),
          ),
        );

      case 'image_decoded':
        return _DecodedImagePreview(
          backend: widget.controller.backend,
          filePath: info.path,
        );

      case 'svg':
        return InteractiveViewer(
          minScale: 0.2,
          maxScale: 8.0,
          child: Center(
            child: Padding(
              padding: const EdgeInsets.all(24),
              child: SvgPicture.file(
                File(info.path),
                fit: BoxFit.contain,
              ),
            ),
          ),
        );

      case 'video':
        return _MediaPlaybackPane(filePath: info.path, isVideo: true);

      case 'audio':
        return _MediaPlaybackPane(filePath: info.path, isVideo: false);

      case 'pdf':
        return _PdfPreviewPane(
          backend: widget.controller.backend,
          info: info,
        );

      case 'office_coming_soon':
        return _OfficeFallbackCard(filePath: info.path);

      case 'markdown':
      case 'csv':
      case 'json':
      case 'code':
      case 'text':
        return FutureBuilder<TextPreviewModel>(
          future: widget.controller.backend.readTextPreview(info.path),
          builder: (context, snap) {
            if (!snap.hasData) {
              if (snap.hasError) {
                return Center(child: Text('Read error: ${snap.error}'));
              }
              return const Center(child: CircularProgressIndicator());
            }
            final textModel = snap.data!;
            switch (info.previewKind) {
              case 'markdown':
                return MarkdownPreviewPane(
                  preview: textModel,
                  truncatedNotice: l10n.truncatedPreviewNotice,
                );
              case 'csv':
                return CsvTablePreviewPane(
                  preview: textModel,
                  truncatedNotice: l10n.truncatedPreviewNotice,
                );
              case 'json':
                return JsonPreviewPane(
                  preview: textModel,
                  truncatedNotice: l10n.truncatedPreviewNotice,
                );
              default:
                return SyntaxCodePreview(
                  source: textModel.text,
                  language: info.languageHint,
                  truncated: textModel.truncated,
                  truncatedNotice: l10n.truncatedPreviewNotice,
                );
            }
          },
        );

      default:
        return Center(
          child: Padding(
            padding: const EdgeInsets.all(32),
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                Icon(
                  info.previewKind == 'archive'
                      ? Icons.folder_zip_outlined
                      : Icons.insert_drive_file_outlined,
                  size: 56,
                  color: Theme.of(context).colorScheme.primary,
                ),
                const SizedBox(height: 14),
                Text(
                  info.previewKind == 'archive'
                      ? 'Archive package — select an Archive tool on the right to inspect or extract.'
                      : 'Binary file — inspect metadata on the right or open in a compatible tool.',
                  textAlign: TextAlign.center,
                  style: Theme.of(context).textTheme.titleMedium,
                ),
                const SizedBox(height: 16),
                FilledButton.tonalIcon(
                  onPressed: () => openInSystemApp(info.path),
                  icon: const Icon(Icons.open_in_new_rounded, size: 18),
                  label: Text(l10n.openInSystemAppButton),
                ),
              ],
            ),
          ),
        );
    }
  }
}

class _DecodedImagePreview extends StatelessWidget {
  final ToolCenterBackend backend;
  final String filePath;

  const _DecodedImagePreview({required this.backend, required this.filePath});

  @override
  Widget build(BuildContext context) {
    return FutureBuilder<Uint8List>(
      future: backend.renderDecodedImagePreview(filePath, maxDimension: 2048),
      builder: (context, snap) {
        if (!snap.hasData) {
          if (snap.hasError) {
            return Center(
              child: Padding(
                padding: const EdgeInsets.all(24),
                child: Text('Could not decode image: ${snap.error}'),
              ),
            );
          }
          return const Center(child: CircularProgressIndicator());
        }
        return InteractiveViewer(
          minScale: 0.2,
          maxScale: 8.0,
          child: Center(
            child: Image.memory(snap.data!, fit: BoxFit.contain),
          ),
        );
      },
    );
  }
}

class _PdfPreviewPane extends StatefulWidget {
  final ToolCenterBackend backend;
  final FileInspectionModel info;

  const _PdfPreviewPane({required this.backend, required this.info});

  @override
  State<_PdfPreviewPane> createState() => _PdfPreviewPaneState();
}

class _PdfPreviewPaneState extends State<_PdfPreviewPane> {
  int _pageIndex = 0;
  int _pageCount = 1;
  int _targetWidth = 1100;
  String? _password;
  bool _needsPassword = false;
  final TextEditingController _pwController = TextEditingController();
  Future<Uint8List>? _pageFuture;

  @override
  void initState() {
    super.initState();
    _pageCount = (widget.info.pageCount ?? 1).clamp(1, 100000);
    _needsPassword = widget.info.pdfEncrypted;
    if (!_needsPassword) {
      _loadPage();
    }
  }

  @override
  void dispose() {
    _pwController.dispose();
    super.dispose();
  }

  void _loadPage() {
    setState(() {
      _pageFuture = widget.backend.renderPdfPagePreview(
        widget.info.path,
        pageIndex: _pageIndex,
        targetWidth: _targetWidth,
        password: _password,
      );
    });
  }

  Future<void> _unlockWithPassword() async {
    final entered = _pwController.text;
    try {
      final count = await widget.backend.getPdfPageCount(
        widget.info.path,
        password: entered,
      );
      setState(() {
        _password = entered;
        _needsPassword = false;
        _pageCount = count.clamp(1, 100000);
        _pageIndex = 0;
      });
      _loadPage();
    } catch (e) {
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text('Incorrect password: $e')),
      );
    }
  }

  @override
  Widget build(BuildContext context) {
    if (_needsPassword) {
      return Center(
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: 380),
          child: Padding(
            padding: const EdgeInsets.all(24),
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                const Icon(Icons.lock_outline_rounded, size: 48),
                const SizedBox(height: 12),
                Text(
                  'Password-Protected PDF',
                  style: Theme.of(context).textTheme.titleLarge,
                ),
                const SizedBox(height: 12),
                TextField(
                  controller: _pwController,
                  obscureText: true,
                  decoration: const InputDecoration(
                    labelText: 'Enter PDF Password',
                  ),
                  onSubmitted: (_) => _unlockWithPassword(),
                ),
                const SizedBox(height: 14),
                FilledButton(
                  onPressed: _unlockWithPassword,
                  child: const Text('Unlock Preview'),
                ),
              ],
            ),
          ),
        ),
      );
    }

    return Column(
      children: [
        Padding(
          padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 8),
          child: Row(
            mainAxisAlignment: MainAxisAlignment.center,
            children: [
              IconButton(
                tooltip: 'Previous Page',
                onPressed: _pageIndex > 0
                    ? () {
                        _pageIndex--;
                        _loadPage();
                      }
                    : null,
                icon: const Icon(Icons.chevron_left_rounded),
              ),
              Text(
                'Page ${_pageIndex + 1} of $_pageCount',
                style: const TextStyle(fontWeight: FontWeight.w600),
              ),
              IconButton(
                tooltip: 'Next Page',
                onPressed: _pageIndex < _pageCount - 1
                    ? () {
                        _pageIndex++;
                        _loadPage();
                      }
                    : null,
                icon: const Icon(Icons.chevron_right_rounded),
              ),
              const SizedBox(width: 24),
              IconButton(
                tooltip: 'Zoom Out',
                onPressed: _targetWidth > 500
                    ? () {
                        _targetWidth = (_targetWidth - 250).clamp(500, 2600);
                        _loadPage();
                      }
                    : null,
                icon: const Icon(Icons.zoom_out_rounded),
              ),
              IconButton(
                tooltip: 'Zoom In',
                onPressed: _targetWidth < 2600
                    ? () {
                        _targetWidth = (_targetWidth + 250).clamp(500, 2600);
                        _loadPage();
                      }
                    : null,
                icon: const Icon(Icons.zoom_in_rounded),
              ),
            ],
          ),
        ),
        const Divider(),
        Expanded(
          child: FutureBuilder<Uint8List>(
            future: _pageFuture,
            builder: (context, snap) {
              if (!snap.hasData) {
                if (snap.hasError) {
                  final errText = snap.error.toString();
                  if (errText.toLowerCase().contains('password')) {
                    WidgetsBinding.instance.addPostFrameCallback((_) {
                      if (mounted && !_needsPassword) {
                        setState(() => _needsPassword = true);
                      }
                    });
                  }
                  return Center(
                    child: Padding(
                      padding: const EdgeInsets.all(24),
                      child: Text('PDF preview error: $errText'),
                    ),
                  );
                }
                return const Center(child: CircularProgressIndicator());
              }
              return InteractiveViewer(
                minScale: 0.5,
                maxScale: 5.0,
                child: SingleChildScrollView(
                  padding: const EdgeInsets.all(20),
                  child: Center(
                    child: DecoratedBox(
                      decoration: BoxDecoration(
                        boxShadow: [
                          BoxShadow(
                            color: Colors.black.withValues(alpha: 0.16),
                            blurRadius: 16,
                            offset: const Offset(0, 4),
                          ),
                        ],
                      ),
                      child: Image.memory(snap.data!, fit: BoxFit.contain),
                    ),
                  ),
                ),
              );
            },
          ),
        ),
      ],
    );
  }
}

class _MediaPlaybackPane extends StatefulWidget {
  final String filePath;
  final bool isVideo;

  const _MediaPlaybackPane({required this.filePath, required this.isVideo});

  @override
  State<_MediaPlaybackPane> createState() => _MediaPlaybackPaneState();
}

class _MediaPlaybackPaneState extends State<_MediaPlaybackPane> {
  Player? _player;
  VideoController? _videoController;
  String? _initError;
  bool _playing = false;
  Duration _position = Duration.zero;
  Duration _duration = Duration.zero;
  final List<StreamSubscription<dynamic>> _subs = [];

  @override
  void initState() {
    super.initState();
    _initPlayer();
  }

  Future<void> _initPlayer() async {
    try {
      final player = Player();
      _player = player;
      if (widget.isVideo) {
        _videoController = VideoController(player);
      }
      _subs.add(
        player.stream.playing.listen((p) {
          if (mounted) setState(() => _playing = p);
        }),
      );
      _subs.add(
        player.stream.position.listen((pos) {
          if (mounted) setState(() => _position = pos);
        }),
      );
      _subs.add(
        player.stream.duration.listen((dur) {
          if (mounted) setState(() => _duration = dur);
        }),
      );
      await player.open(Media(widget.filePath), play: false);
    } catch (e) {
      if (mounted) {
        setState(() => _initError = e.toString());
      }
    }
  }

  @override
  void dispose() {
    for (final s in _subs) {
      s.cancel();
    }
    _player?.dispose();
    super.dispose();
  }

  String _fmt(Duration d) {
    final mins = d.inMinutes.remainder(60).toString().padLeft(2, '0');
    final secs = d.inSeconds.remainder(60).toString().padLeft(2, '0');
    if (d.inHours > 0) {
      return '${d.inHours}:$mins:$secs';
    }
    return '$mins:$secs';
  }

  @override
  Widget build(BuildContext context) {
    if (_initError != null) {
      return Center(
        child: Padding(
          padding: const EdgeInsets.all(24),
          child: Text('Media playback unavailable: $_initError'),
        ),
      );
    }

    if (widget.isVideo && _videoController != null) {
      return Video(controller: _videoController!);
    }

    final maxMs = _duration.inMilliseconds.clamp(1, 86400000).toDouble();
    final posMs = _position.inMilliseconds.clamp(0, maxMs.toInt()).toDouble();

    return Center(
      child: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: 460),
        child: Padding(
          padding: const EdgeInsets.all(28),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              Container(
                padding: const EdgeInsets.all(24),
                decoration: BoxDecoration(
                  color: Theme.of(context).colorScheme.primaryContainer,
                  shape: BoxShape.circle,
                ),
                child: Icon(
                  Icons.graphic_eq_rounded,
                  size: 48,
                  color: Theme.of(context).colorScheme.onPrimaryContainer,
                ),
              ),
              const SizedBox(height: 24),
              Slider(
                min: 0,
                max: maxMs,
                value: posMs,
                onChanged: (v) {
                  _player?.seek(Duration(milliseconds: v.round()));
                },
              ),
              Row(
                mainAxisAlignment: MainAxisAlignment.spaceBetween,
                children: [
                  Text(_fmt(_position)),
                  IconButton.filled(
                    iconSize: 32,
                    onPressed: () => _player?.playOrPause(),
                    icon: Icon(
                      _playing ? Icons.pause_rounded : Icons.play_arrow_rounded,
                    ),
                  ),
                  Text(_fmt(_duration)),
                ],
              ),
            ],
          ),
        ),
      ),
    );
  }
}

class _OfficeFallbackCard extends StatelessWidget {
  final String filePath;

  const _OfficeFallbackCard({required this.filePath});

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context);
    final theme = Theme.of(context);
    return Center(
      child: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: 480),
        child: Padding(
          padding: const EdgeInsets.all(32),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              Container(
                padding: const EdgeInsets.all(18),
                decoration: BoxDecoration(
                  color: theme.colorScheme.secondaryContainer,
                  shape: BoxShape.circle,
                ),
                child: Icon(
                  Icons.description_outlined,
                  size: 44,
                  color: theme.colorScheme.onSecondaryContainer,
                ),
              ),
              const SizedBox(height: 20),
              Text(
                l10n.officePreviewComingSoonTitle,
                style: theme.textTheme.headlineSmall?.copyWith(
                  fontWeight: FontWeight.w700,
                ),
              ),
              const SizedBox(height: 10),
              Text(
                l10n.officePreviewComingSoonBody,
                textAlign: TextAlign.center,
                style: theme.textTheme.bodyMedium?.copyWith(
                  color: theme.colorScheme.onSurfaceVariant,
                ),
              ),
              const SizedBox(height: 24),
              FilledButton.icon(
                onPressed: () => openInSystemApp(filePath),
                icon: const Icon(Icons.open_in_new_rounded, size: 18),
                label: Text(l10n.openInSystemAppButton),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

class _MetadataSidebar extends StatelessWidget {
  final FileInspectionModel info;
  final AppController controller;

  const _MetadataSidebar({required this.info, required this.controller});

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context);
    final theme = Theme.of(context);
    final ext = info.extension.toLowerCase();

    final compatibleTools = controller.tools.where((t) {
      if (t.inputSpec.acceptedExtensions.isEmpty) return true;
      return ext.isNotEmpty && t.inputSpec.acceptedExtensions.contains(ext);
    }).toList();

    return Container(
      width: 340,
      decoration: BoxDecoration(
        color: theme.cardTheme.color,
        border: Border(left: BorderSide(color: theme.dividerColor)),
      ),
      child: ListView(
        padding: const EdgeInsets.all(18),
        children: [
          Text(
            l10n.metadataPanelTitle,
            style: theme.textTheme.titleMedium?.copyWith(
              fontWeight: FontWeight.w700,
            ),
          ),
          const SizedBox(height: 12),
          for (final item in info.metadata) ...[
            Padding(
              padding: const EdgeInsets.symmetric(vertical: 6),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    item.label,
                    style: theme.textTheme.labelMedium?.copyWith(
                      color: theme.colorScheme.onSurfaceVariant,
                    ),
                  ),
                  const SizedBox(height: 2),
                  SelectableText(
                    item.value,
                    style: theme.textTheme.bodyMedium?.copyWith(
                      fontWeight: FontWeight.w600,
                    ),
                  ),
                ],
              ),
            ),
            const Divider(),
          ],
          if (compatibleTools.isNotEmpty) ...[
            const SizedBox(height: 16),
            Text(
              l10n.compatibleToolsTitle,
              style: theme.textTheme.titleSmall?.copyWith(
                fontWeight: FontWeight.w700,
              ),
            ),
            const SizedBox(height: 10),
            for (final tool in compatibleTools.take(8))
              Padding(
                padding: const EdgeInsets.only(bottom: 8),
                child: OutlinedButton.icon(
                  style: OutlinedButton.styleFrom(
                    alignment: Alignment.centerLeft,
                  ),
                  onPressed: () =>
                      controller.openTool(tool, initialFiles: [info.path]),
                  icon: Icon(resolveToolIcon(tool.icon), size: 18),
                  label: Text(tool.name, overflow: TextOverflow.ellipsis),
                ),
              ),
          ],
        ],
      ),
    );
  }
}

class _PreviewErrorState extends StatelessWidget {
  final String message;
  final VoidCallback onBack;

  const _PreviewErrorState({required this.message, required this.onBack});

  @override
  Widget build(BuildContext context) {
    return Center(
      child: Padding(
        padding: const EdgeInsets.all(32),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(
              Icons.broken_image_outlined,
              size: 48,
              color: Theme.of(context).colorScheme.error,
            ),
            const SizedBox(height: 12),
            SelectableText(message, textAlign: TextAlign.center),
            const SizedBox(height: 16),
            OutlinedButton(
              onPressed: onBack,
              child: const Text('Back to Home'),
            ),
          ],
        ),
      ),
    );
  }
}
