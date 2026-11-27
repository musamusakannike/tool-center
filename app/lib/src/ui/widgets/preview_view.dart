import 'package:material_ui/material_ui.dart';
import 'package:path/path.dart' as p;

import '../../../l10n/app_localizations.dart';
import '../../services/core_service.dart';
import '../../state/app_controller.dart';

class FilePreviewView extends StatelessWidget {
  final AppController controller;
  final String filePath;

  const FilePreviewView({
    super.key,
    required this.controller,
    required this.filePath,
  });

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context);
    return Padding(
      padding: const EdgeInsets.all(24),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              OutlinedButton.icon(
                onPressed: controller.goHome,
                icon: const Icon(Icons.arrow_back_rounded, size: 18),
                label: Text(l10n.backToHomeButton),
              ),
              const SizedBox(width: 16),
              Expanded(
                child: Text(
                  p.basename(filePath),
                  style: Theme.of(
                    context,
                  ).textTheme.titleLarge?.copyWith(fontWeight: FontWeight.w700),
                ),
              ),
              OutlinedButton.icon(
                onPressed: () => revealInSystemFolder(filePath),
                icon: const Icon(Icons.folder_open_rounded, size: 18),
                label: Text(l10n.revealInFolderButton),
              ),
            ],
          ),
          const SizedBox(height: 24),
          Expanded(
            child: Card(
              child: Center(
                child: SelectableText(filePath),
              ),
            ),
          ),
        ],
      ),
    );
  }
}
