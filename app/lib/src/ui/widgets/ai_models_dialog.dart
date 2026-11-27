import 'package:material_ui/material_ui.dart';

import '../../../l10n/app_localizations.dart';
import '../../models/tool_models.dart';
import '../../services/core_service.dart';

Future<void> showAiModelsDialog({
  required BuildContext context,
  required ToolCenterBackend backend,
}) {
  return showDialog<void>(
    context: context,
    builder: (ctx) => _AiModelsDialog(backend: backend),
  );
}

class _AiModelsDialog extends StatefulWidget {
  final ToolCenterBackend backend;

  const _AiModelsDialog({required this.backend});

  @override
  State<_AiModelsDialog> createState() => _AiModelsDialogState();
}

class _AiModelsDialogState extends State<_AiModelsDialog> {
  late Future<(List<AiModelStatusModel>, List<FlaggedExcludedModelInfo>)>
  _future;

  @override
  void initState() {
    super.initState();
    _future = widget.backend.listAiModels();
  }

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context);
    final theme = Theme.of(context);

    return AlertDialog(
      title: Text(l10n.aiModelsDialogTitle),
      content: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: 640, maxHeight: 500),
        child: FutureBuilder(
          future: _future,
          builder: (context, snapshot) {
            if (!snapshot.hasData) {
              if (snapshot.hasError) {
                return Text('Unable to load model catalog: ${snapshot.error}');
              }
              return const Center(child: CircularProgressIndicator());
            }
            final (models, flagged) = snapshot.data!;
            return SingleChildScrollView(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    l10n.aiModelsDialogSubtitle,
                    style: theme.textTheme.bodyMedium?.copyWith(
                      color: theme.colorScheme.onSurfaceVariant,
                    ),
                  ),
                  const SizedBox(height: 16),
                  Text(
                    'Permissive ONNX Models (Planned for v2)',
                    style: theme.textTheme.titleSmall?.copyWith(
                      fontWeight: FontWeight.w700,
                    ),
                  ),
                  const SizedBox(height: 8),
                  for (final m in models) ...[
                    Card(
                      child: ListTile(
                        leading: const Icon(Icons.memory_rounded),
                        title: Text(
                          m.name,
                          style: const TextStyle(fontWeight: FontWeight.w600),
                        ),
                        subtitle: Text(
                          '${m.description}\nLicense: ${m.spdxLicense} • Size: ${formatByteSize(m.sizeBytes)}',
                        ),
                        isThreeLine: true,
                        trailing: Chip(
                          label: Text(
                            m.installed ? 'Cached' : 'v2 On-Demand',
                            style: const TextStyle(fontSize: 12),
                          ),
                        ),
                      ),
                    ),
                    const SizedBox(height: 8),
                  ],
                  if (flagged.isNotEmpty) ...[
                    const SizedBox(height: 12),
                    Text(
                      'Flagged & Excluded Non-Commercial Models (Never Used)',
                      style: theme.textTheme.titleSmall?.copyWith(
                        fontWeight: FontWeight.w700,
                        color: theme.colorScheme.error,
                      ),
                    ),
                    const SizedBox(height: 8),
                    for (final f in flagged) ...[
                      Card(
                        child: ListTile(
                          leading: Icon(
                            Icons.block_rounded,
                            color: theme.colorScheme.error,
                          ),
                          title: Text('${f.name} (${f.upstreamLicense})'),
                          subtitle: Text(f.reason),
                        ),
                      ),
                      const SizedBox(height: 6),
                    ],
                  ],
                ],
              ),
            );
          },
        ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.of(context).pop(),
          child: const Text('Close'),
        ),
      ],
    );
  }
}
