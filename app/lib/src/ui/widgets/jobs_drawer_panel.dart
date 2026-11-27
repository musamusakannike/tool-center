import 'package:material_ui/material_ui.dart';

import '../../../l10n/app_localizations.dart';
import '../../models/tool_models.dart';
import '../../services/core_service.dart';
import '../../state/app_controller.dart';

class JobsDrawerPanel extends StatelessWidget {
  final AppController controller;

  const JobsDrawerPanel({super.key, required this.controller});

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context);
    final jobs = controller.jobs;
    final theme = Theme.of(context);

    return Container(
      width: 360,
      decoration: BoxDecoration(
        color: theme.cardTheme.color,
        border: Border(left: BorderSide(color: theme.dividerColor)),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Padding(
            padding: const EdgeInsets.fromLTRB(16, 14, 10, 14),
            child: Row(
              children: [
                const Icon(Icons.bolt_rounded, size: 20),
                const SizedBox(width: 8),
                Expanded(
                  child: Text(
                    l10n.jobsPanelTitle,
                    style: theme.textTheme.titleMedium?.copyWith(
                      fontWeight: FontWeight.w700,
                    ),
                  ),
                ),
                if (jobs.any((j) => !j.status.isActive))
                  TextButton(
                    onPressed: controller.clearFinishedJobs,
                    child: Text(l10n.clearFinishedJobs),
                  ),
                IconButton(
                  tooltip: 'Close panel',
                  icon: const Icon(Icons.close_rounded, size: 20),
                  onPressed: () => controller.toggleJobsDrawer(false),
                ),
              ],
            ),
          ),
          const Divider(),
          Expanded(
            child: jobs.isEmpty
                ? Center(
                    child: Padding(
                      padding: const EdgeInsets.all(24),
                      child: Text(
                        l10n.jobsEmpty,
                        textAlign: TextAlign.center,
                        style: theme.textTheme.bodyMedium?.copyWith(
                          color: theme.colorScheme.onSurfaceVariant,
                        ),
                      ),
                    ),
                  )
                : ListView.separated(
                    padding: const EdgeInsets.all(12),
                    itemCount: jobs.length,
                    separatorBuilder: (_, _) => const SizedBox(height: 10),
                    itemBuilder: (context, idx) {
                      final job = jobs[idx];
                      return _JobCard(
                        job: job,
                        onCancel: () => controller.cancelJob(job.jobId),
                        onPreviewOutput: (path) => controller.openPreview(path),
                      );
                    },
                  ),
          ),
        ],
      ),
    );
  }
}

class _JobCard extends StatelessWidget {
  final JobSnapshotModel job;
  final VoidCallback onCancel;
  final ValueChanged<String> onPreviewOutput;

  const _JobCard({
    required this.job,
    required this.onCancel,
    required this.onPreviewOutput,
  });

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context);
    final theme = Theme.of(context);
    final scheme = theme.colorScheme;

    Color badgeColor;
    IconData badgeIcon;
    switch (job.status) {
      case JobStatusId.queued:
      case JobStatusId.running:
        badgeColor = scheme.primary;
        badgeIcon = Icons.sync_rounded;
      case JobStatusId.completed:
        badgeColor = Colors.green;
        badgeIcon = Icons.check_circle_outline_rounded;
      case JobStatusId.cancelled:
        badgeColor = Colors.orange;
        badgeIcon = Icons.cancel_outlined;
      case JobStatusId.failed:
        badgeColor = scheme.error;
        badgeIcon = Icons.error_outline_rounded;
    }

    return Card(
      child: Padding(
        padding: const EdgeInsets.all(12),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                Icon(badgeIcon, size: 18, color: badgeColor),
                const SizedBox(width: 8),
                Expanded(
                  child: Text(
                    job.toolName,
                    style: theme.textTheme.titleSmall?.copyWith(
                      fontWeight: FontWeight.w600,
                    ),
                  ),
                ),
                if (job.status.isActive)
                  TextButton(
                    onPressed: onCancel,
                    child: Text(l10n.cancelJobButton),
                  ),
              ],
            ),
            const SizedBox(height: 6),
            Text(
              job.errorMessage ?? job.summary ?? job.stepMessage,
              style: theme.textTheme.bodySmall?.copyWith(
                color: job.status == JobStatusId.failed
                    ? scheme.error
                    : scheme.onSurfaceVariant,
              ),
            ),
            if (job.status.isActive) ...[
              const SizedBox(height: 8),
              LinearProgressIndicator(
                value: job.progress > 0 ? job.progress : null,
                borderRadius: BorderRadius.circular(4),
              ),
            ],
            if (job.outputs.isNotEmpty) ...[
              const SizedBox(height: 8),
              Wrap(
                spacing: 8,
                runSpacing: 6,
                children: [
                  OutlinedButton.icon(
                    onPressed: () =>
                        revealInSystemFolder(job.outputs.first.path),
                    icon: const Icon(Icons.folder_open_rounded, size: 16),
                    label: Text(l10n.revealInFolderButton),
                  ),
                  TextButton.icon(
                    onPressed: () => onPreviewOutput(job.outputs.first.path),
                    icon: const Icon(Icons.visibility_outlined, size: 16),
                    label: Text(l10n.openOutputButton),
                  ),
                ],
              ),
            ],
          ],
        ),
      ),
    );
  }
}
