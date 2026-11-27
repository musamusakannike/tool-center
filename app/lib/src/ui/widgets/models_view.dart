import 'package:file_picker/file_picker.dart';
import 'package:material_ui/material_ui.dart';
import 'package:url_launcher/url_launcher.dart';

import '../../../l10n/app_localizations.dart';
import '../../models/tool_models.dart';
import '../../services/core_service.dart';
import '../../state/app_controller.dart';
import '../theme/app_theme.dart';

/// Dedicated full-page AI Models & Runtime Manager screen.
class ModelsView extends StatefulWidget {
  final AppController controller;

  const ModelsView({super.key, required this.controller});

  @override
  State<ModelsView> createState() => _ModelsViewState();
}

class _ModelsViewState extends State<ModelsView> {
  String _selectedTaskFilter = 'all';
  String? _actionBannerMessage;
  bool _actionIsError = false;

  Future<void> _pickCustomFolder() async {
    final uri = await FilePicker.pickDirectory();
    final path = uri?.toFilePath();
    if (path != null && path.isNotEmpty) {
      try {
        await widget.controller.setCustomAiModelsDir(path);
        if (!mounted) return;
        setState(() {
          _actionIsError = false;
          _actionBannerMessage = 'Model directory updated to $path';
        });
      } catch (e) {
        if (!mounted) return;
        setState(() {
          _actionIsError = true;
          _actionBannerMessage = e.toString();
        });
      }
    }
  }

  Future<void> _importOfflineModel(AiModelStatusModel model) async {
    String? chosenPath;
    if (model.isMultiFilePack) {
      final uri = await FilePicker.pickDirectory();
      chosenPath = uri?.toFilePath();
    } else {
      final file = await FilePicker.pickFile(
        type: FileType.custom,
        allowedExtensions: const ['onnx'],
      );
      chosenPath = file?.path;
    }
    if (chosenPath == null || chosenPath.isEmpty) return;

    try {
      await widget.controller.importModelFromPath(
        modelId: model.id,
        sourcePath: chosenPath,
      );
      if (!mounted) return;
      setState(() {
        _actionIsError = false;
        _actionBannerMessage =
            'Verified SHA-256 and imported ${model.displayName}.';
      });
    } catch (e) {
      if (!mounted) return;
      setState(() {
        _actionIsError = true;
        _actionBannerMessage = e.toString();
      });
    }
  }

  String _taskDisplayLabel(String task) {
    switch (task) {
      case 'ocr':
        return 'OCR & Searchable PDF';
      case 'background_removal':
        return 'Background Removal';
      case 'upscale':
        return 'AI Upscaling';
      default:
        return 'All AI Tasks';
    }
  }

  String _providerLabel(String p) {
    switch (p) {
      case 'auto':
        return 'Auto (Hardware Acceleration with CPU Fallback)';
      case 'cpu':
        return 'CPU Only (Always Compatible)';
      case 'coreml':
        return 'Apple CoreML (Neural Engine / GPU)';
      case 'directml':
        return 'Windows DirectML (DirectX 12 GPU)';
      case 'cuda':
        return 'NVIDIA CUDA';
      default:
        return p.toUpperCase();
    }
  }

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context);
    final theme = Theme.of(context);
    final scheme = theme.colorScheme;
    final catalog = widget.controller.aiCatalog;

    if (catalog == null) {
      return const Center(child: CircularProgressIndicator());
    }

    final filteredModels = catalog.models.where((m) {
      if (_selectedTaskFilter == 'all') return true;
      return m.task == _selectedTaskFilter;
    }).toList();

    final providerChoices = <String>{
      'auto',
      ...catalog.settings.availableProviders,
    }.toList();

    return SingleChildScrollView(
      padding: const EdgeInsets.all(24),
      child: Center(
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: 980),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              // Header
              Row(
                children: [
                  OutlinedButton.icon(
                    onPressed: widget.controller.goHome,
                    icon: const Icon(Icons.arrow_back_rounded, size: 18),
                    label: Text(l10n.backToHomeButton),
                  ),
                  const SizedBox(width: 16),
                  Icon(
                    Icons.auto_awesome_rounded,
                    size: 28,
                    color: scheme.primary,
                  ),
                  const SizedBox(width: 12),
                  Expanded(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(
                          l10n.aiModelsDialogTitle,
                          style: theme.textTheme.headlineSmall?.copyWith(
                            fontWeight: FontWeight.w700,
                          ),
                        ),
                        Text(
                          l10n.aiModelsDialogSubtitle,
                          style: theme.textTheme.bodyMedium?.copyWith(
                            color: scheme.onSurfaceVariant,
                          ),
                        ),
                      ],
                    ),
                  ),
                ],
              ),
              const SizedBox(height: 20),

              if (_actionBannerMessage != null) ...[
                Card(
                  color: _actionIsError
                      ? scheme.errorContainer
                      : scheme.secondaryContainer,
                  child: Padding(
                    padding: const EdgeInsets.symmetric(
                      horizontal: 16,
                      vertical: 12,
                    ),
                    child: Row(
                      children: [
                        Icon(
                          _actionIsError
                              ? Icons.error_outline_rounded
                              : Icons.verified_rounded,
                          color: _actionIsError
                              ? scheme.onErrorContainer
                              : scheme.onSecondaryContainer,
                        ),
                        const SizedBox(width: 10),
                        Expanded(
                          child: SelectableText(
                            _actionBannerMessage!,
                            style: TextStyle(
                              color: _actionIsError
                                  ? scheme.onErrorContainer
                                  : scheme.onSecondaryContainer,
                            ),
                          ),
                        ),
                        IconButton(
                          icon: const Icon(Icons.close_rounded, size: 18),
                          onPressed: () =>
                              setState(() => _actionBannerMessage = null),
                        ),
                      ],
                    ),
                  ),
                ),
                const SizedBox(height: 16),
              ],

              // Storage & Acceleration Settings Card
              Card(
                child: Padding(
                  padding: const EdgeInsets.all(20),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Row(
                        children: [
                          const Icon(Icons.folder_special_outlined, size: 20),
                          const SizedBox(width: 10),
                          Expanded(
                            child: Column(
                              crossAxisAlignment: CrossAxisAlignment.start,
                              children: [
                                Text(
                                  'Model Storage Directory',
                                  style: theme.textTheme.titleSmall?.copyWith(
                                    fontWeight: FontWeight.w700,
                                  ),
                                ),
                                const SizedBox(height: 2),
                                SelectableText(
                                  catalog.settings.modelsDir,
                                  style: theme.textTheme.bodySmall?.copyWith(
                                    color: scheme.onSurfaceVariant,
                                  ),
                                ),
                              ],
                            ),
                          ),
                          if (catalog.settings.isCustomDir) ...[
                            TextButton(
                              onPressed: () =>
                                  widget.controller.setCustomAiModelsDir(null),
                              child: const Text('Reset Default'),
                            ),
                            const SizedBox(width: 6),
                          ],
                          OutlinedButton.icon(
                            onPressed: () => revealInSystemFolder(
                              catalog.settings.modelsDir,
                            ),
                            icon: const Icon(
                              Icons.folder_open_rounded,
                              size: 16,
                            ),
                            label: const Text('Open'),
                          ),
                          const SizedBox(width: 8),
                          OutlinedButton.icon(
                            onPressed: _pickCustomFolder,
                            icon: const Icon(Icons.drive_file_move_outline, size: 16),
                            label: const Text('Change Folder...'),
                          ),
                        ],
                      ),
                      const Padding(
                        padding: EdgeInsets.symmetric(vertical: 14),
                        child: Divider(),
                      ),
                      Row(
                        children: [
                          const Icon(Icons.speed_rounded, size: 20),
                          const SizedBox(width: 10),
                          Expanded(
                            child: Column(
                              crossAxisAlignment: CrossAxisAlignment.start,
                              children: [
                                Text(
                                  'Inference Execution Device',
                                  style: theme.textTheme.titleSmall?.copyWith(
                                    fontWeight: FontWeight.w700,
                                  ),
                                ),
                                Text(
                                  'CPU always works offline; hardware accelerators automatically fall back to CPU if unavailable.',
                                  style: theme.textTheme.bodySmall?.copyWith(
                                    color: scheme.onSurfaceVariant,
                                  ),
                                ),
                              ],
                            ),
                          ),
                          const SizedBox(width: 12),
                          DropdownButton<String>(
                            value: providerChoices.contains(
                              catalog.settings.devicePreference,
                            )
                                ? catalog.settings.devicePreference
                                : 'auto',
                            items: [
                              for (final p in providerChoices)
                                DropdownMenuItem(
                                  value: p,
                                  child: Text(_providerLabel(p)),
                                ),
                            ],
                            onChanged: (next) {
                              if (next != null) {
                                widget.controller.setAiDevicePreference(next);
                              }
                            },
                          ),
                        ],
                      ),
                    ],
                  ),
                ),
              ),
              const SizedBox(height: 20),

              // Task Filter Chips
              Wrap(
                spacing: 8,
                runSpacing: 8,
                children: [
                  for (final t in const [
                    'all',
                    'ocr',
                    'background_removal',
                    'upscale',
                  ])
                    ChoiceChip(
                      label: Text(_taskDisplayLabel(t)),
                      selected: _selectedTaskFilter == t,
                      onSelected: (_) =>
                          setState(() => _selectedTaskFilter = t),
                    ),
                ],
              ),
              const SizedBox(height: 16),

              // Model Cards
              for (final model in filteredModels) ...[
                _ModelCard(
                  model: model,
                  taskLabel: _taskDisplayLabel(model.task),
                  onDownload: () =>
                      widget.controller.startModelDownload(model.id),
                  onCancel: () =>
                      widget.controller.cancelModelDownload(model.id),
                  onImport: () => _importOfflineModel(model),
                  onDelete: () => widget.controller.removeModel(model.id),
                ),
                const SizedBox(height: 12),
              ],

              if (catalog.flaggedExcluded.isNotEmpty) ...[
                const SizedBox(height: 20),
                ExpansionTile(
                  leading: Icon(Icons.gpp_maybe_outlined, color: scheme.error),
                  title: Text(
                    'License Gate: Excluded Non-Commercial / Ambiguous Models (${catalog.flaggedExcluded.length})',
                    style: theme.textTheme.titleSmall?.copyWith(
                      fontWeight: FontWeight.w700,
                    ),
                  ),
                  subtitle: const Text(
                    'Explicitly blocked to guarantee 100% permissive (MIT / Apache-2.0 / BSD) commercial-safe usage.',
                  ),
                  children: [
                    for (final f in catalog.flaggedExcluded)
                      ListTile(
                        dense: true,
                        leading: Icon(
                          Icons.block_rounded,
                          size: 18,
                          color: scheme.error,
                        ),
                        title: Text('${f.name} — ${f.upstreamLicense}'),
                        subtitle: Text(f.reason),
                      ),
                  ],
                ),
              ],
            ],
          ),
        ),
      ),
    );
  }
}

class _ModelCard extends StatelessWidget {
  final AiModelStatusModel model;
  final String taskLabel;
  final VoidCallback onDownload;
  final VoidCallback onCancel;
  final VoidCallback onImport;
  final VoidCallback onDelete;

  const _ModelCard({
    required this.model,
    required this.taskLabel,
    required this.onDownload,
    required this.onCancel,
    required this.onImport,
    required this.onDelete,
  });

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final scheme = theme.colorScheme;
    final sizeLabel = formatByteSize(model.sizeBytes);
    final isDownloading = model.state == ModelInstallStateId.downloading;
    final hasPartial = !model.installed && model.resumableBytes > 0;

    final (statusText, statusColor) = switch (model.state) {
      ModelInstallStateId.ready => ('Ready', Colors.green.shade700),
      ModelInstallStateId.downloading => ('Downloading', scheme.primary),
      ModelInstallStateId.failed => ('Failed', scheme.error),
      ModelInstallStateId.notInstalled => (
        hasPartial
            ? 'Paused (${formatByteSize(model.resumableBytes)} saved)'
            : 'Not Installed',
        scheme.onSurfaceVariant,
      ),
    };

    return Card(
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(AppTheme.radiusMd),
        side: BorderSide(
          color: model.installed
              ? Colors.green.withValues(alpha: 0.45)
              : theme.dividerColor,
        ),
      ),
      child: Padding(
        padding: const EdgeInsets.all(18),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Container(
                  padding: const EdgeInsets.all(10),
                  decoration: BoxDecoration(
                    color: scheme.primaryContainer.withValues(alpha: 0.7),
                    borderRadius: BorderRadius.circular(10),
                  ),
                  child: Icon(
                    switch (model.task) {
                      'ocr' => Icons.document_scanner_outlined,
                      'background_removal' => Icons.auto_fix_high_rounded,
                      _ => Icons.hd_outlined,
                    },
                    color: scheme.onPrimaryContainer,
                  ),
                ),
                const SizedBox(width: 14),
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Wrap(
                        spacing: 8,
                        runSpacing: 6,
                        crossAxisAlignment: WrapCrossAlignment.center,
                        children: [
                          Text(
                            model.displayName,
                            style: theme.textTheme.titleMedium?.copyWith(
                              fontWeight: FontWeight.w700,
                            ),
                          ),
                          if (model.defaultForTask)
                            Chip(
                              visualDensity: VisualDensity.compact,
                              label: const Text(
                                'Recommended',
                                style: TextStyle(fontSize: 11.5),
                              ),
                            ),
                          Chip(
                            visualDensity: VisualDensity.compact,
                            label: Text(
                              statusText,
                              style: TextStyle(
                                fontSize: 11.5,
                                fontWeight: FontWeight.w600,
                                color: statusColor,
                              ),
                            ),
                          ),
                        ],
                      ),
                      const SizedBox(height: 4),
                      Text(
                        model.description,
                        style: theme.textTheme.bodyMedium?.copyWith(
                          color: scheme.onSurfaceVariant,
                        ),
                      ),
                      const SizedBox(height: 8),
                      Wrap(
                        spacing: 14,
                        runSpacing: 4,
                        children: [
                          Text(
                            'Task: $taskLabel',
                            style: theme.textTheme.labelMedium,
                          ),
                          Text(
                            'Download Size: $sizeLabel',
                            style: theme.textTheme.labelMedium?.copyWith(
                              fontWeight: FontWeight.w700,
                            ),
                          ),
                          Text(
                            'License: ${model.spdxLicense}',
                            style: theme.textTheme.labelMedium,
                          ),
                          if (model.sourceUrl.isNotEmpty)
                            InkWell(
                              onTap: () =>
                                  launchUrl(Uri.parse(model.sourceUrl)),
                              child: Text(
                                'Source & Attribution ↗',
                                style: theme.textTheme.labelMedium?.copyWith(
                                  color: scheme.primary,
                                  decoration: TextDecoration.underline,
                                ),
                              ),
                            ),
                        ],
                      ),
                    ],
                  ),
                ),
                const SizedBox(width: 12),
                Wrap(
                  spacing: 8,
                  runSpacing: 8,
                  children: [
                    if (isDownloading)
                      OutlinedButton.icon(
                        onPressed: onCancel,
                        icon: const Icon(Icons.stop_circle_outlined, size: 18),
                        label: const Text('Cancel'),
                      )
                    else if (model.installed)
                      OutlinedButton.icon(
                        onPressed: onDelete,
                        icon: const Icon(Icons.delete_outline_rounded, size: 18),
                        label: const Text('Remove'),
                      )
                    else ...[
                      OutlinedButton.icon(
                        onPressed: onImport,
                        icon: const Icon(Icons.upload_file_outlined, size: 18),
                        label: Text(
                          model.isMultiFilePack
                              ? 'Import Folder...'
                              : 'Import .onnx...',
                        ),
                      ),
                      FilledButton.icon(
                        onPressed: onDownload,
                        icon: const Icon(Icons.download_rounded, size: 18),
                        label: Text(
                          hasPartial
                              ? 'Resume ($sizeLabel)'
                              : 'Download ($sizeLabel)',
                        ),
                      ),
                    ],
                  ],
                ),
              ],
            ),
            if (isDownloading) ...[
              const SizedBox(height: 14),
              Row(
                children: [
                  Expanded(
                    child: Text(
                      model.statusMessage ?? 'Downloading...',
                      style: theme.textTheme.bodySmall,
                    ),
                  ),
                  Text(
                    '${formatByteSize(model.downloadedBytes)} / $sizeLabel (${(model.progress * 100).toStringAsFixed(0)}%)',
                    style: theme.textTheme.bodySmall?.copyWith(
                      fontWeight: FontWeight.w600,
                    ),
                  ),
                ],
              ),
              const SizedBox(height: 6),
              LinearProgressIndicator(
                value: model.progress > 0 ? model.progress : null,
                borderRadius: BorderRadius.circular(4),
              ),
            ],
            if (model.errorMessage != null && !isDownloading) ...[
              const SizedBox(height: 10),
              SelectableText(
                model.errorMessage!,
                style: TextStyle(color: scheme.error, fontSize: 12.5),
              ),
            ],
          ],
        ),
      ),
    );
  }
}

/// Reusable friendly one-click prompt displayed inside any AI tool when its selected model
/// is not yet installed locally. Always displays the exact download size upfront.
class MissingModelPromptCard extends StatelessWidget {
  final AiModelStatusModel model;
  final AppController controller;

  const MissingModelPromptCard({
    super.key,
    required this.model,
    required this.controller,
  });

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final scheme = theme.colorScheme;
    final sizeText = formatByteSize(model.sizeBytes);
    final isDownloading = model.state == ModelInstallStateId.downloading;
    final hasPartial = !model.installed && model.resumableBytes > 0;

    return Card(
      color: scheme.primaryContainer.withValues(alpha: 0.35),
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(AppTheme.radiusMd),
        side: BorderSide(color: scheme.primary.withValues(alpha: 0.6)),
      ),
      child: Padding(
        padding: const EdgeInsets.all(18),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                Icon(Icons.downloading_rounded, color: scheme.primary),
                const SizedBox(width: 10),
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        'Model required: ${model.displayName}',
                        style: theme.textTheme.titleSmall?.copyWith(
                          fontWeight: FontWeight.w700,
                        ),
                      ),
                      const SizedBox(height: 2),
                      Text(
                        'Downloaded once and runs 100% offline afterward • Size: $sizeText • License: ${model.spdxLicense}',
                        style: theme.textTheme.bodySmall?.copyWith(
                          color: scheme.onSurfaceVariant,
                        ),
                      ),
                    ],
                  ),
                ),
                const SizedBox(width: 12),
                if (isDownloading)
                  OutlinedButton.icon(
                    onPressed: () => controller.cancelModelDownload(model.id),
                    icon: const Icon(Icons.stop_circle_outlined, size: 18),
                    label: const Text('Cancel'),
                  )
                else ...[
                  TextButton(
                    onPressed: controller.openModelsScreen,
                    child: const Text('All Models'),
                  ),
                  const SizedBox(width: 8),
                  FilledButton.icon(
                    onPressed: () => controller.startModelDownload(model.id),
                    icon: const Icon(Icons.download_rounded, size: 18),
                    label: Text(
                      hasPartial
                          ? 'Resume Download ($sizeText)'
                          : 'Download Model ($sizeText)',
                    ),
                  ),
                ],
              ],
            ),
            if (isDownloading) ...[
              const SizedBox(height: 12),
              Row(
                children: [
                  Expanded(
                    child: Text(
                      model.statusMessage ?? 'Downloading model...',
                      style: theme.textTheme.bodySmall,
                    ),
                  ),
                  Text(
                    '${formatByteSize(model.downloadedBytes)} / $sizeText',
                    style: theme.textTheme.bodySmall?.copyWith(
                      fontWeight: FontWeight.w600,
                    ),
                  ),
                ],
              ),
              const SizedBox(height: 6),
              LinearProgressIndicator(
                value: model.progress > 0 ? model.progress : null,
                borderRadius: BorderRadius.circular(4),
              ),
            ],
            if (model.errorMessage != null && !isDownloading) ...[
              const SizedBox(height: 8),
              SelectableText(
                model.errorMessage!,
                style: TextStyle(color: scheme.error, fontSize: 12.5),
              ),
            ],
          ],
        ),
      ),
    );
  }
}
