import 'package:desktop_drop/desktop_drop.dart';
import 'package:file_picker/file_picker.dart';
import 'package:material_ui/material_ui.dart';
import 'package:path/path.dart' as p;

import '../../../l10n/app_localizations.dart';
import '../../models/tool_models.dart';
import '../../state/app_controller.dart';
import '../theme/app_theme.dart';

class HomeView extends StatefulWidget {
  final AppController controller;

  const HomeView({super.key, required this.controller});

  @override
  State<HomeView> createState() => _HomeViewState();
}

class _HomeViewState extends State<HomeView> {
  bool _isDragging = false;

  Future<void> _pickFileForPreview() async {
    final file = await FilePicker.pickFile();
    final path = file?.path;
    if (path != null && path.isNotEmpty) {
      widget.controller.openPreview(path);
    }
  }

  String _categoryLabel(AppLocalizations l10n, ToolCategoryId? cat) {
    if (cat == null) return l10n.categoryAll;
    switch (cat) {
      case ToolCategoryId.ai:
        return l10n.categoryAi;
      case ToolCategoryId.images:
        return l10n.categoryImages;
      case ToolCategoryId.video:
        return l10n.categoryVideo;
      case ToolCategoryId.audio:
        return l10n.categoryAudio;
      case ToolCategoryId.pdf:
        return l10n.categoryPdf;
      case ToolCategoryId.files:
        return l10n.categoryFiles;
      case ToolCategoryId.convert:
        return l10n.categoryConvert;
    }
  }

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context);
    final theme = Theme.of(context);
    final scheme = theme.colorScheme;
    final tools = widget.controller.filteredTools;
    final recent = widget.controller.recentFiles;

    return DropTarget(
      onDragEntered: (_) => setState(() => _isDragging = true),
      onDragExited: (_) => setState(() => _isDragging = false),
      onDragDone: (details) {
        setState(() => _isDragging = false);
        final first = details.files.firstOrNull?.path;
        if (first != null && first.isNotEmpty) {
          widget.controller.openPreview(first);
        }
      },
      child: SingleChildScrollView(
        padding: const EdgeInsets.all(24),
        child: Center(
          child: ConstrainedBox(
            constraints: const BoxConstraints(maxWidth: 1180),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                // Prominent Drop Zone
                AnimatedContainer(
                  duration: const Duration(milliseconds: 160),
                  padding: const EdgeInsets.symmetric(
                    horizontal: 24,
                    vertical: 28,
                  ),
                  decoration: BoxDecoration(
                    color: _isDragging
                        ? scheme.primaryContainer.withValues(alpha: 0.45)
                        : theme.cardTheme.color,
                    borderRadius: BorderRadius.circular(AppTheme.radiusLg),
                    border: Border.all(
                      color: _isDragging ? scheme.primary : theme.dividerColor,
                      width: _isDragging ? 2 : 1,
                    ),
                  ),
                  child: Row(
                    children: [
                      Container(
                        padding: const EdgeInsets.all(14),
                        decoration: BoxDecoration(
                          color: scheme.primaryContainer,
                          borderRadius: BorderRadius.circular(12),
                        ),
                        child: Icon(
                          Icons.file_present_rounded,
                          size: 30,
                          color: scheme.onPrimaryContainer,
                        ),
                      ),
                      const SizedBox(width: 18),
                      Expanded(
                        child: Column(
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [
                            Text(
                              l10n.dropZoneTitle,
                              style: theme.textTheme.titleLarge?.copyWith(
                                fontWeight: FontWeight.w700,
                              ),
                            ),
                            const SizedBox(height: 4),
                            Text(
                              l10n.dropZoneSubtitle,
                              style: theme.textTheme.bodyMedium?.copyWith(
                                color: scheme.onSurfaceVariant,
                              ),
                            ),
                          ],
                        ),
                      ),
                      const SizedBox(width: 16),
                      FilledButton.icon(
                        onPressed: _pickFileForPreview,
                        icon: const Icon(Icons.folder_open_rounded, size: 18),
                        label: Text(l10n.browseFilesButton),
                      ),
                    ],
                  ),
                ),
                const SizedBox(height: 24),

                // Category Filter Bar
                Wrap(
                  spacing: 8,
                  runSpacing: 8,
                  children: [
                    for (final cat in <ToolCategoryId?>[
                      null,
                      ToolCategoryId.ai,
                      ToolCategoryId.images,
                      ToolCategoryId.video,
                      ToolCategoryId.audio,
                      ToolCategoryId.pdf,
                      ToolCategoryId.files,
                      ToolCategoryId.convert,
                    ])
                      ChoiceChip(
                        label: Text(_categoryLabel(l10n, cat)),
                        selected: widget.controller.selectedCategory == cat,
                        onSelected: (_) => widget.controller.setCategory(cat),
                      ),
                  ],
                ),
                const SizedBox(height: 20),

                // Tool Grid
                if (tools.isEmpty)
                  Card(
                    child: Padding(
                      padding: const EdgeInsets.all(32),
                      child: Center(
                        child: Text(
                          widget.controller.tools.isEmpty
                              ? l10n.noToolsRegisteredYet
                              : l10n.commandPaletteEmpty,
                          style: theme.textTheme.bodyMedium?.copyWith(
                            color: scheme.onSurfaceVariant,
                          ),
                        ),
                      ),
                    ),
                  )
                else
                  LayoutBuilder(
                    builder: (context, constraints) {
                      final cols = constraints.maxWidth >= 960
                          ? 4
                          : constraints.maxWidth >= 680
                          ? 3
                          : constraints.maxWidth >= 440
                          ? 2
                          : 1;
                      return GridView.builder(
                        shrinkWrap: true,
                        physics: const NeverScrollableScrollPhysics(),
                        itemCount: tools.length,
                        gridDelegate: SliverGridDelegateWithFixedCrossAxisCount(
                          crossAxisCount: cols,
                          crossAxisSpacing: 14,
                          mainAxisSpacing: 14,
                          mainAxisExtent: 136,
                        ),
                        itemBuilder: (context, index) {
                          final tool = tools[index];
                          return _ToolCard(
                            tool: tool,
                            onTap: () => widget.controller.openTool(tool),
                            onDropFiles: (paths) => widget.controller.openTool(
                              tool,
                              initialFiles: paths,
                            ),
                          );
                        },
                      );
                    },
                  ),

                const SizedBox(height: 28),

                // Recent Files Section
                Row(
                  children: [
                    Text(
                      l10n.recentFilesTitle,
                      style: theme.textTheme.titleMedium?.copyWith(
                        fontWeight: FontWeight.w700,
                      ),
                    ),
                    const Spacer(),
                    if (recent.isNotEmpty)
                      TextButton(
                        onPressed: widget.controller.clearRecentFiles,
                        child: Text(l10n.clearRecentButton),
                      ),
                  ],
                ),
                const SizedBox(height: 10),
                if (recent.isEmpty)
                  Card(
                    child: Padding(
                      padding: const EdgeInsets.all(20),
                      child: Text(
                        l10n.recentFilesEmpty,
                        style: theme.textTheme.bodyMedium?.copyWith(
                          color: scheme.onSurfaceVariant,
                        ),
                      ),
                    ),
                  )
                else
                  Card(
                    child: Column(
                      children: [
                        for (int i = 0; i < recent.length && i < 8; i++) ...[
                          ListTile(
                            leading: const Icon(
                              Icons.history_rounded,
                              size: 20,
                            ),
                            title: Text(p.basename(recent[i])),
                            subtitle: Text(
                              recent[i],
                              maxLines: 1,
                              overflow: TextOverflow.ellipsis,
                            ),
                            trailing: const Icon(
                              Icons.chevron_right_rounded,
                              size: 20,
                            ),
                            onTap: () =>
                                widget.controller.openPreview(recent[i]),
                          ),
                          if (i < recent.length - 1 && i < 7) const Divider(),
                        ],
                      ],
                    ),
                  ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

class _ToolCard extends StatefulWidget {
  final ToolDescriptorModel tool;
  final VoidCallback onTap;
  final ValueChanged<List<String>> onDropFiles;

  const _ToolCard({
    required this.tool,
    required this.onTap,
    required this.onDropFiles,
  });

  @override
  State<_ToolCard> createState() => _ToolCardState();
}

class _ToolCardState extends State<_ToolCard> {
  bool _hovering = false;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final scheme = theme.colorScheme;

    return DropTarget(
      onDragDone: (details) {
        final paths = details.files.map((f) => f.path).toList();
        if (paths.isNotEmpty) widget.onDropFiles(paths);
      },
      child: MouseRegion(
        onEnter: (_) => setState(() => _hovering = true),
        onExit: (_) => setState(() => _hovering = false),
        child: Card(
          shape: RoundedRectangleBorder(
            borderRadius: BorderRadius.circular(AppTheme.radiusMd),
            side: BorderSide(
              color: _hovering ? scheme.primary : theme.dividerColor,
              width: _hovering ? 1.5 : 1,
            ),
          ),
          child: InkWell(
            borderRadius: BorderRadius.circular(AppTheme.radiusMd),
            onTap: widget.onTap,
            child: Padding(
              padding: const EdgeInsets.all(16),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Row(
                    children: [
                      Container(
                        padding: const EdgeInsets.all(8),
                        decoration: BoxDecoration(
                          color: scheme.primaryContainer.withValues(alpha: 0.7),
                          borderRadius: BorderRadius.circular(8),
                        ),
                        child: Icon(
                          resolveToolIcon(widget.tool.icon),
                          size: 20,
                          color: scheme.onPrimaryContainer,
                        ),
                      ),
                      const Spacer(),
                      Text(
                        widget.tool.category.name.toUpperCase(),
                        style: theme.textTheme.labelSmall?.copyWith(
                          color: scheme.onSurfaceVariant,
                          letterSpacing: 0.6,
                        ),
                      ),
                    ],
                  ),
                  const Spacer(),
                  Text(
                    widget.tool.name,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: theme.textTheme.titleMedium?.copyWith(
                      fontWeight: FontWeight.w700,
                    ),
                  ),
                  const SizedBox(height: 4),
                  Text(
                    widget.tool.description,
                    maxLines: 2,
                    overflow: TextOverflow.ellipsis,
                    style: theme.textTheme.bodySmall?.copyWith(
                      color: scheme.onSurfaceVariant,
                    ),
                  ),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }
}
