// ignore: unused_import
import 'package:intl/intl.dart' as intl;

import 'app_localizations.dart';

// ignore_for_file: type=lint

/// The translations for English (`en`).
class AppLocalizationsEn extends AppLocalizations {
  AppLocalizationsEn([String locale = 'en']) : super(locale);

  @override
  String get appTitle => 'Tool Center';

  @override
  String get appTagline => 'Fast, offline file preview & conversion toolbox';

  @override
  String get searchToolsHint => 'Search tools or press Ctrl+K...';

  @override
  String get commandPalettePlaceholder =>
      'Type a tool name, format, or action...';

  @override
  String get commandPaletteEmpty => 'No matching tools found.';

  @override
  String get dropZoneTitle => 'Drop any file here to preview or pick a tool';

  @override
  String get dropZoneSubtitle =>
      'Works 100% offline • Originals are never overwritten';

  @override
  String get browseFilesButton => 'Open File to Preview';

  @override
  String get recentFilesTitle => 'Recent Files';

  @override
  String get recentFilesEmpty =>
      'Files you preview or process will appear here for quick access.';

  @override
  String get clearRecentButton => 'Clear';

  @override
  String get categoryAll => 'All Tools';

  @override
  String get categoryAi => 'AI';

  @override
  String get categoryImages => 'Images';

  @override
  String get categoryVideo => 'Video';

  @override
  String get categoryAudio => 'Audio';

  @override
  String get categoryPdf => 'PDF';

  @override
  String get categoryFiles => 'Files';

  @override
  String get categoryConvert => 'Convert';

  @override
  String get jobsPanelTitle => 'Background Jobs';

  @override
  String get jobsEmpty => 'No active or recent jobs. Select any tool to begin.';

  @override
  String get clearFinishedJobs => 'Clear Finished';

  @override
  String get cancelJobButton => 'Cancel';

  @override
  String get revealInFolderButton => 'Reveal in Folder';

  @override
  String get openOutputButton => 'Preview Output';

  @override
  String get pickFilesStep => '1. Pick Files';

  @override
  String get addFilesButton => 'Add Files';

  @override
  String get addFolderButton => 'Add Folder';

  @override
  String get clearSelectionButton => 'Clear All';

  @override
  String get optionsStep => '2. Options';

  @override
  String get outputLocationLabel => 'Save output to';

  @override
  String get outputNextToSource => 'Next to original file (safe suffix)';

  @override
  String get chooseOutputFolderButton => 'Choose Folder...';

  @override
  String get resetToDefaultButton => 'Use Default';

  @override
  String get runToolButton => 'Run Tool';

  @override
  String get runningStatus => 'Running...';

  @override
  String get resultsStep => '3. Results';

  @override
  String get aiModelsButton => 'Models';

  @override
  String get aiModelsDialogTitle => 'AI Models & Runtime';

  @override
  String get aiModelsDialogSubtitle =>
      'Models are never bundled in the installer. Download on demand or import offline — verified with SHA-256 and restricted to permissive (MIT / Apache-2.0 / BSD) licenses.';

  @override
  String get themeTooltip => 'Switch theme (System / Light / Dark)';

  @override
  String get backToHomeButton => 'All Tools';

  @override
  String get noToolsRegisteredYet =>
      'Tools load dynamically from the Rust registry.';

  @override
  String get officePreviewComingSoonTitle => 'Preview coming soon';

  @override
  String get officePreviewComingSoonBody =>
      'Full offline rendering for Office documents (DOCX, XLSX, PPTX) is planned for a future release. You can open this file directly in your default system app below.';

  @override
  String get openInSystemAppButton => 'Open in System App';

  @override
  String get metadataPanelTitle => 'File Metadata';

  @override
  String get compatibleToolsTitle => 'Quick Actions with This File';

  @override
  String get truncatedPreviewNotice =>
      'Showing the first 256 KB so large files open instantly without high memory use.';
}
