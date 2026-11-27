import 'dart:convert';

import 'package:desktop_drop/desktop_drop.dart';
import 'package:file_picker/file_picker.dart';
import 'package:material_ui/material_ui.dart';
import 'package:path/path.dart' as p;

import '../../../l10n/app_localizations.dart';
import '../../models/tool_models.dart';
import '../../services/core_service.dart';
import '../../state/app_controller.dart';
import '../theme/app_theme.dart';
import 'ai_result_widgets.dart';
import 'models_view.dart';
import 'schema_options_form.dart';

class ToolWorkspaceView extends StatefulWidget {
  final AppController controller;
  final ToolDescriptorModel tool;
  final List<String> initialFiles;

  const ToolWorkspaceView({
    super.key,
    required this.controller,
    required this.tool,
    this.initialFiles = const [],
  });

  @override
  State<ToolWorkspaceView> createState() => _ToolWorkspaceViewState();
}

class _ToolWorkspaceViewState extends State<ToolWorkspaceView> {
  late List<String> _selectedPaths;
  late Map<String, dynamic> _optionValues;
  String? _customOutputDir;
  String? _activeJobId;
  String? _submissionError;
  bool _isDragging = false;

  @override
  void initState() {
    super.initState();
    _selectedPaths = List<String>.from(widget.initialFiles);
    _optionValues = widget.tool.defaultOptionsMap();
  }

  @override
  void didUpdateWidget(covariant ToolWorkspaceView oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.tool.id != widget.tool.id) {
      _selectedPaths = List<String>.from(widget.initialFiles);
      _optionValues = widget.tool.defaultOptionsMap();
      _customOutputDir = null;
      _activeJobId = null;
      _submissionError = null;
    }
  }

  JobSnapshotModel? get _currentJob {
    final id = _activeJobId;
    if (id == null) return null;
    for (final j in widget.controller.jobs) {
      if (j.jobId == id) return j;
    }
    return null;
  }

  AiModelStatusModel? get _missingRequiredModel {
    final selectedModelId = _optionValues['model_id']?.toString();
    if (selectedModelId == null ||
        selectedModelId.isEmpty ||
        selectedModelId == 'lanczos_fallback') {
      return null;
    }
    final status = widget.controller.aiCatalog?.findById(selectedModelId);
    if (status != null && !status.installed) {
      return status;
    }
    return null;
  }

  Future<void> _pickFiles() async {
    final spec = widget.tool.inputSpec;
    final allowMultiple = spec.maxFiles == null || spec.maxFiles! > 1;
    final type =
        spec.acceptedExtensions.isEmpty ? FileType.any : FileType.custom;
    final exts =
        spec.acceptedExtensions.isEmpty ? null : spec.acceptedExtensions;

    if (allowMultiple) {
      final files = await FilePicker.pickFiles(
        type: type,
        allowedExtensions: exts,
      );
      final paths = files
          .map((f) => f.path)
          .whereType<String>()
          .where((s) => s.isNotEmpty)
          .toList();
      if (paths.isNotEmpty) _addPaths(paths);
    } else {
      final file = await FilePicker.pickFile(
        type: type,
        allowedExtensions: exts,
      );
      final path = file?.path;
      if (path != null && path.isNotEmpty) _addPaths([path]);
    }
  }

  Future<void> _pickDirectory() async {
    final uri = await FilePicker.pickDirectory();
    final dir = uri?.toFilePath();
    if (dir != null && dir.isNotEmpty) {
      _addPaths([dir]);
    }
  }

  void _addPaths(List<String> incoming) {
    setState(() {
      _submissionError = null;
      if (widget.tool.inputSpec.maxFiles == 1 && incoming.isNotEmpty) {
        _selectedPaths = [incoming.first];
      } else {
        for (final path in incoming) {
          if (!_selectedPaths.contains(path)) {
            _selectedPaths.add(path);
          }
        }
      }
    });
  }

  Future<void> _chooseOutputFolder() async {
    final uri = await FilePicker.pickDirectory();
    final dir = uri?.toFilePath();
    if (dir != null && dir.isNotEmpty) {
      setState(() {
        _customOutputDir = dir;
      });
    }
  }

  Future<void> _runTool() async {
    setState(() {
      _submissionError = null;
    });
    try {
      final jobId = await widget.controller.submitToolJob(
        toolId: widget.tool.id,
        inputs: _selectedPaths,
        options: _optionValues,
        outputDir: _customOutputDir,
      );
      setState(() {
        _activeJobId = jobId;
      });
    } catch (e) {
      setState(() {
        _submissionError = e.toString();
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context);
    final theme = Theme.of(context);
    final scheme = theme.colorScheme;
    final job = _currentJob;
    final isRunning = job != null && job.status.isActive;

    return DropTarget(
      onDragEntered: (_) => setState(() => _isDragging = true),
      onDragExited: (_) => setState(() => _isDragging = false),
      onDragDone: (details) {
        setState(() => _isDragging = false);
        _addPaths(details.files.map((x) => x.path).toList());
      },
      child: SingleChildScrollView(
        padding: const EdgeInsets.all(24),
        child: Center(
          child: ConstrainedBox(
            constraints: const BoxConstraints(maxWidth: 860),
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
                      resolveToolIcon(widget.tool.icon),
                      size: 28,
                      color: scheme.primary,
                    ),
                    const SizedBox(width: 12),
                    Expanded(
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Text(
                            widget.tool.name,
                            style: theme.textTheme.headlineSmall?.copyWith(
                              fontWeight: FontWeight.w700,
                            ),
                          ),
                          Text(
                            widget.tool.description,
                            style: theme.textTheme.bodyMedium?.copyWith(
                              color: scheme.onSurfaceVariant,
                            ),
                          ),
                        ],
                      ),
                    ),
                  ],
                ),
                const SizedBox(height: 24),

                // Step 1: Pick Files
                Card(
                  shape: RoundedRectangleBorder(
                    borderRadius: BorderRadius.circular(AppTheme.radiusMd),
                    side: BorderSide(
                      color: _isDragging ? scheme.primary : theme.dividerColor,
                      width: _isDragging ? 2 : 1,
                    ),
                  ),
                  child: Padding(
                    padding: const EdgeInsets.all(20),
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Row(
                          children: [
                            Expanded(
                              child: Text(
                                l10n.pickFilesStep,
                                style: theme.textTheme.titleMedium?.copyWith(
                                  fontWeight: FontWeight.w700,
                                ),
                              ),
                            ),
                            OutlinedButton.icon(
                              onPressed: _pickFiles,
                              icon: const Icon(Icons.add_rounded, size: 18),
                              label: Text(l10n.addFilesButton),
                            ),
                            if (widget.tool.inputSpec.allowDirectories) ...[
                              const SizedBox(width: 8),
                              OutlinedButton.icon(
                                onPressed: _pickDirectory,
                                icon: const Icon(
                                  Icons.create_new_folder_outlined,
                                  size: 18,
                                ),
                                label: Text(l10n.addFolderButton),
                              ),
                            ],
                            if (_selectedPaths.isNotEmpty) ...[
                              const SizedBox(width: 8),
                              TextButton(
                                onPressed: () =>
                                    setState(() => _selectedPaths.clear()),
                                child: Text(l10n.clearSelectionButton),
                              ),
                            ],
                          ],
                        ),
                        const SizedBox(height: 12),
                        if (_selectedPaths.isEmpty)
                          InkWell(
                            borderRadius: BorderRadius.circular(
                              AppTheme.radiusSm,
                            ),
                            onTap: _pickFiles,
                            child: Container(
                              width: double.infinity,
                              padding: const EdgeInsets.symmetric(
                                vertical: 28,
                                horizontal: 16,
                              ),
                              decoration: BoxDecoration(
                                borderRadius: BorderRadius.circular(
                                  AppTheme.radiusSm,
                                ),
                                border: Border.all(
                                  color: scheme.outlineVariant,
                                ),
                              ),
                              child: Column(
                                children: [
                                  Icon(
                                    Icons.upload_file_rounded,
                                    size: 32,
                                    color: scheme.primary,
                                  ),
                                  const SizedBox(height: 8),
                                  Text(
                                    'Drop files here or click to choose',
                                    style: theme.textTheme.titleSmall,
                                  ),
                                  if (widget
                                      .tool
                                      .inputSpec
                                      .acceptedExtensions
                                      .isNotEmpty) ...[
                                    const SizedBox(height: 4),
                                    Text(
                                      'Accepted: ${widget.tool.inputSpec.acceptedExtensions.map((e) => '.$e').join(', ')}',
                                      style: theme.textTheme.bodySmall
                                          ?.copyWith(
                                            color: scheme.onSurfaceVariant,
                                          ),
                                    ),
                                  ],
                                ],
                              ),
                            ),
                          )
                        else
                          Column(
                            children: [
                              for (
                                int i = 0;
                                i < _selectedPaths.length;
                                i++
                              ) ...[
                                ListTile(
                                  dense: true,
                                  contentPadding: EdgeInsets.zero,
                                  leading: const Icon(
                                    Icons.insert_drive_file_outlined,
                                    size: 20,
                                  ),
                                  title: Text(p.basename(_selectedPaths[i])),
                                  subtitle: Text(
                                    _selectedPaths[i],
                                    maxLines: 1,
                                    overflow: TextOverflow.ellipsis,
                                  ),
                                  trailing: Row(
                                    mainAxisSize: MainAxisSize.min,
                                    children: [
                                      if (_selectedPaths.length > 1) ...[
                                        IconButton(
                                          tooltip: 'Move up',
                                          icon: const Icon(
                                            Icons.arrow_upward_rounded,
                                            size: 18,
                                          ),
                                          onPressed: i > 0
                                              ? () {
                                                  setState(() {
                                                    final item = _selectedPaths
                                                        .removeAt(i);
                                                    _selectedPaths.insert(
                                                      i - 1,
                                                      item,
                                                    );
                                                  });
                                                }
                                              : null,
                                        ),
                                        IconButton(
                                          tooltip: 'Move down',
                                          icon: const Icon(
                                            Icons.arrow_downward_rounded,
                                            size: 18,
                                          ),
                                          onPressed:
                                              i < _selectedPaths.length - 1
                                              ? () {
                                                  setState(() {
                                                    final item = _selectedPaths
                                                        .removeAt(i);
                                                    _selectedPaths.insert(
                                                      i + 1,
                                                      item,
                                                    );
                                                  });
                                                }
                                              : null,
                                        ),
                                      ],
                                      IconButton(
                                        tooltip: 'Remove',
                                        icon: const Icon(
                                          Icons.close_rounded,
                                          size: 18,
                                        ),
                                        onPressed: () {
                                          setState(() {
                                            _selectedPaths.removeAt(i);
                                          });
                                        },
                                      ),
                                    ],
                                  ),
                                ),
                                if (i < _selectedPaths.length - 1)
                                  const Divider(),
                              ],
                            ],
                          ),
                      ],
                    ),
                  ),
                ),
                const SizedBox(height: 16),

                // Step 2: Options + Non-destructive Output Folder
                Card(
                  child: Padding(
                    padding: const EdgeInsets.all(20),
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(
                          l10n.optionsStep,
                          style: theme.textTheme.titleMedium?.copyWith(
                            fontWeight: FontWeight.w700,
                          ),
                        ),
                        const SizedBox(height: 16),
                        SchemaOptionsForm(
                          schema: widget.tool.optionsSchema,
                          values: _optionValues,
                          onChanged: (next) =>
                              setState(() => _optionValues = next),
                        ),
                        const SizedBox(height: 20),
                        const Divider(),
                        const SizedBox(height: 14),
                        Row(
                          children: [
                            const Icon(Icons.shield_outlined, size: 18),
                            const SizedBox(width: 8),
                            Expanded(
                              child: Column(
                                crossAxisAlignment: CrossAxisAlignment.start,
                                children: [
                                  Text(
                                    l10n.outputLocationLabel,
                                    style: theme.textTheme.titleSmall?.copyWith(
                                      fontWeight: FontWeight.w600,
                                    ),
                                  ),
                                  Text(
                                    _customOutputDir ?? l10n.outputNextToSource,
                                    style: theme.textTheme.bodySmall?.copyWith(
                                      color: scheme.onSurfaceVariant,
                                    ),
                                  ),
                                ],
                              ),
                            ),
                            if (_customOutputDir != null)
                              TextButton(
                                onPressed: () =>
                                    setState(() => _customOutputDir = null),
                                child: Text(l10n.resetToDefaultButton),
                              ),
                            OutlinedButton(
                              onPressed: _chooseOutputFolder,
                              child: Text(l10n.chooseOutputFolderButton),
                            ),
                          ],
                        ),
                      ],
                    ),
                  ),
                ),
                const SizedBox(height: 16),

                if (_submissionError != null) ...[
                  Card(
                    color: scheme.errorContainer,
                    child: Padding(
                      padding: const EdgeInsets.all(14),
                      child: Row(
                        children: [
                          Icon(
                            Icons.error_outline_rounded,
                            color: scheme.onErrorContainer,
                          ),
                          const SizedBox(width: 10),
                          Expanded(
                            child: Text(
                              _submissionError!,
                              style: TextStyle(color: scheme.onErrorContainer),
                            ),
                          ),
                        ],
                      ),
                    ),
                  ),
                  const SizedBox(height: 16),
                ],

                if (_missingRequiredModel != null) ...[
                  MissingModelPromptCard(
                    model: _missingRequiredModel!,
                    controller: widget.controller,
                  ),
                  const SizedBox(height: 16),
                ],

                // Run / Cancel Action Row
                Row(
                  mainAxisAlignment: MainAxisAlignment.end,
                  children: [
                    if (isRunning) ...[
                      OutlinedButton.icon(
                        onPressed: () =>
                            widget.controller.cancelJob(job.jobId),
                        icon: const Icon(Icons.stop_circle_outlined, size: 18),
                        label: Text(l10n.cancelJobButton),
                      ),
                      const SizedBox(width: 12),
                    ],
                    FilledButton.icon(
                      onPressed: isRunning ||
                              _selectedPaths.isEmpty ||
                              _missingRequiredModel != null
                          ? null
                          : _runTool,
                      icon: isRunning
                          ? const SizedBox(
                              width: 16,
                              height: 16,
                              child: CircularProgressIndicator(strokeWidth: 2),
                            )
                          : const Icon(Icons.play_arrow_rounded),
                      label: Text(
                        isRunning ? l10n.runningStatus : l10n.runToolButton,
                      ),
                    ),
                  ],
                ),

                // Step 3: Live Progress & Results
                if (job != null) ...[
                  const SizedBox(height: 20),
                  _JobResultSection(
                    job: job,
                    backend: widget.controller.backend,
                    onPreviewOutput: widget.controller.openPreview,
                  ),
                ],
              ],
            ),
          ),
        ),
      ),
    );
  }
}

class _JobResultSection extends StatelessWidget {
  final JobSnapshotModel job;
  final ToolCenterBackend backend;
  final ValueChanged<String> onPreviewOutput;

  const _JobResultSection({
    required this.job,
    required this.backend,
    required this.onPreviewOutput,
  });

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context);
    final theme = Theme.of(context);
    final scheme = theme.colorScheme;

    return Card(
      child: Padding(
        padding: const EdgeInsets.all(20),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                Text(
                  l10n.resultsStep,
                  style: theme.textTheme.titleMedium?.copyWith(
                    fontWeight: FontWeight.w700,
                  ),
                ),
                const Spacer(),
                if (job.durationMs != null)
                  Text(
                    '${(job.durationMs! / 1000.0).toStringAsFixed(2)}s',
                    style: theme.textTheme.bodySmall?.copyWith(
                      color: scheme.onSurfaceVariant,
                    ),
                  ),
              ],
            ),
            const SizedBox(height: 12),
            if (job.status.isActive) ...[
              Text(job.stepMessage),
              const SizedBox(height: 8),
              LinearProgressIndicator(
                value: job.progress > 0 ? job.progress : null,
                borderRadius: BorderRadius.circular(4),
              ),
            ] else if (job.status == JobStatusId.failed) ...[
              SelectableText(
                job.errorMessage ?? 'Operation failed.',
                style: TextStyle(
                  color: scheme.error,
                  fontWeight: FontWeight.w600,
                ),
              ),
              if (job.errorDetails != null && job.errorDetails!.isNotEmpty) ...[
                const SizedBox(height: 8),
                ExpansionTile(
                  tilePadding: EdgeInsets.zero,
                  title: const Text('Technical details'),
                  children: [
                    SelectableText(
                      job.errorDetails!,
                      style: theme.textTheme.bodySmall,
                    ),
                  ],
                ),
              ],
            ] else if (job.status == JobStatusId.cancelled) ...[
              const Text('Cancelled — no original files were modified.'),
            ] else ...[
              Row(
                children: [
                  const Icon(
                    Icons.check_circle_rounded,
                    color: Colors.green,
                    size: 20,
                  ),
                  const SizedBox(width: 8),
                  Expanded(
                    child: Text(
                      job.summary ?? 'Completed successfully.',
                      style: const TextStyle(fontWeight: FontWeight.w600),
                    ),
                  ),
                ],
              ),
              if (job.warnings.isNotEmpty) ...[
                const SizedBox(height: 8),
                for (final w in job.warnings)
                  Text(
                    '• $w',
                    style: TextStyle(color: Colors.amber.shade800),
                  ),
              ],
              if (AiJobResultEnhancement.isAiVisualReport(job.reportJson)) ...[
                const SizedBox(height: 16),
                AiJobResultEnhancement(
                  rawReportJson: job.reportJson!,
                  backend: backend,
                ),
              ],
              if (job.outputs.isNotEmpty) ...[
                const SizedBox(height: 14),
                for (final out in job.outputs)
                  ListTile(
                    contentPadding: EdgeInsets.zero,
                    leading: const Icon(Icons.task_alt_rounded),
                    title: Text(out.label),
                    subtitle: Text(
                      _buildSizeSubtitle(out),
                      maxLines: 2,
                      overflow: TextOverflow.ellipsis,
                    ),
                    trailing: Wrap(
                      spacing: 8,
                      children: [
                        OutlinedButton.icon(
                          onPressed: () => revealInSystemFolder(out.path),
                          icon: const Icon(Icons.folder_open_rounded, size: 16),
                          label: Text(l10n.revealInFolderButton),
                        ),
                        FilledButton.tonalIcon(
                          onPressed: () => onPreviewOutput(out.path),
                          icon: const Icon(Icons.visibility_outlined, size: 16),
                          label: Text(l10n.openOutputButton),
                        ),
                      ],
                    ),
                  ),
              ],
              if (job.reportJson != null &&
                  !AiJobResultEnhancement.isAiVisualReport(job.reportJson)) ...[
                const SizedBox(height: 12),
                _StructuredReportView(rawJson: job.reportJson!),
              ],
            ],
          ],
        ),
      ),
    );
  }

  String _buildSizeSubtitle(OutputArtifactModel out) {
    final sizeText = formatByteSize(out.sizeBytes);
    final orig = out.originalSizeBytes;
    if (orig != null && orig > 0 && out.sizeBytes > 0 && orig != out.sizeBytes) {
      final pct = ((1.0 - (out.sizeBytes / orig)) * 100.0);
      if (pct > 0) {
        return '$sizeText (${pct.toStringAsFixed(1)}% smaller) • ${out.path}';
      }
    }
    return '$sizeText • ${out.path}';
  }
}

class _StructuredReportView extends StatelessWidget {
  final String rawJson;

  const _StructuredReportView({required this.rawJson});

  @override
  Widget build(BuildContext context) {
    String formatted = rawJson;
    try {
      final parsed = jsonDecode(rawJson);
      formatted = const JsonEncoder.withIndent('  ').convert(parsed);
    } catch (_) {}

    return Container(
      width: double.infinity,
      padding: const EdgeInsets.all(12),
      decoration: BoxDecoration(
        color: Theme.of(context).colorScheme.surfaceContainerHighest,
        borderRadius: BorderRadius.circular(8),
      ),
      child: SelectableText(
        formatted,
        style: const TextStyle(fontFamily: 'monospace', fontSize: 12.5),
      ),
    );
  }
}
