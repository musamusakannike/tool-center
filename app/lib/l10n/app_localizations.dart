import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:intl/intl.dart' as intl;

import 'app_localizations_en.dart';

// ignore_for_file: type=lint

/// Callers can lookup localized strings with an instance of AppLocalizations
/// returned by `AppLocalizations.of(context)`.
///
/// Applications need to include `AppLocalizations.delegate()` in their app's
/// `localizationDelegates` list, and the locales they support in the app's
/// `supportedLocales` list. For example:
///
/// ```dart
/// import 'l10n/app_localizations.dart';
///
/// return MaterialApp(
///   localizationsDelegates: AppLocalizations.localizationsDelegates,
///   supportedLocales: AppLocalizations.supportedLocales,
///   home: MyApplicationHome(),
/// );
/// ```
///
/// ## Update pubspec.yaml
///
/// Please make sure to update your pubspec.yaml to include the following
/// packages:
///
/// ```yaml
/// dependencies:
///   # Internationalization support.
///   flutter_localizations:
///     sdk: flutter
///   intl: any # Use the pinned version from flutter_localizations
///
///   # Rest of dependencies
/// ```
///
/// ## iOS Applications
///
/// iOS applications define key application metadata, including supported
/// locales, in an Info.plist file that is built into the application bundle.
/// To configure the locales supported by your app, you’ll need to edit this
/// file.
///
/// First, open your project’s ios/Runner.xcworkspace Xcode workspace file.
/// Then, in the Project Navigator, open the Info.plist file under the Runner
/// project’s Runner folder.
///
/// Next, select the Information Property List item, select Add Item from the
/// Editor menu, then select Localizations from the pop-up menu.
///
/// Select and expand the newly-created Localizations item then, for each
/// locale your application supports, add a new item and select the locale
/// you wish to add from the pop-up menu in the Value field. This list should
/// be consistent with the languages listed in the AppLocalizations.supportedLocales
/// property.
abstract class AppLocalizations {
  AppLocalizations(String locale)
    : localeName = intl.Intl.canonicalizedLocale(locale.toString());

  final String localeName;

  static AppLocalizations of(BuildContext context) {
    return Localizations.of<AppLocalizations>(context, AppLocalizations)!;
  }

  static const LocalizationsDelegate<AppLocalizations> delegate =
      _AppLocalizationsDelegate();

  /// A list of this localizations delegate along with the default localizations
  /// delegates.
  ///
  /// Returns a list of localizations delegates containing this delegate along with
  /// GlobalMaterialLocalizations.delegate, GlobalCupertinoLocalizations.delegate,
  /// and GlobalWidgetsLocalizations.delegate.
  ///
  /// Additional delegates can be added by appending to this list in
  /// MaterialApp. This list does not have to be used at all if a custom list
  /// of delegates is preferred or required.
  static const List<LocalizationsDelegate<dynamic>> localizationsDelegates =
      <LocalizationsDelegate<dynamic>>[
        delegate,
        GlobalMaterialLocalizations.delegate,
        GlobalCupertinoLocalizations.delegate,
        GlobalWidgetsLocalizations.delegate,
      ];

  /// A list of this localizations delegate's supported locales.
  static const List<Locale> supportedLocales = <Locale>[Locale('en')];

  /// No description provided for @appTitle.
  ///
  /// In en, this message translates to:
  /// **'Tool Center'**
  String get appTitle;

  /// No description provided for @appTagline.
  ///
  /// In en, this message translates to:
  /// **'Fast, offline file preview & conversion toolbox'**
  String get appTagline;

  /// No description provided for @searchToolsHint.
  ///
  /// In en, this message translates to:
  /// **'Search tools or press Ctrl+K...'**
  String get searchToolsHint;

  /// No description provided for @commandPalettePlaceholder.
  ///
  /// In en, this message translates to:
  /// **'Type a tool name, format, or action...'**
  String get commandPalettePlaceholder;

  /// No description provided for @commandPaletteEmpty.
  ///
  /// In en, this message translates to:
  /// **'No matching tools found.'**
  String get commandPaletteEmpty;

  /// No description provided for @dropZoneTitle.
  ///
  /// In en, this message translates to:
  /// **'Drop any file here to preview or pick a tool'**
  String get dropZoneTitle;

  /// No description provided for @dropZoneSubtitle.
  ///
  /// In en, this message translates to:
  /// **'Works 100% offline • Originals are never overwritten'**
  String get dropZoneSubtitle;

  /// No description provided for @browseFilesButton.
  ///
  /// In en, this message translates to:
  /// **'Open File to Preview'**
  String get browseFilesButton;

  /// No description provided for @recentFilesTitle.
  ///
  /// In en, this message translates to:
  /// **'Recent Files'**
  String get recentFilesTitle;

  /// No description provided for @recentFilesEmpty.
  ///
  /// In en, this message translates to:
  /// **'Files you preview or process will appear here for quick access.'**
  String get recentFilesEmpty;

  /// No description provided for @clearRecentButton.
  ///
  /// In en, this message translates to:
  /// **'Clear'**
  String get clearRecentButton;

  /// No description provided for @categoryAll.
  ///
  /// In en, this message translates to:
  /// **'All Tools'**
  String get categoryAll;

  /// No description provided for @categoryImages.
  ///
  /// In en, this message translates to:
  /// **'Images'**
  String get categoryImages;

  /// No description provided for @categoryVideo.
  ///
  /// In en, this message translates to:
  /// **'Video'**
  String get categoryVideo;

  /// No description provided for @categoryAudio.
  ///
  /// In en, this message translates to:
  /// **'Audio'**
  String get categoryAudio;

  /// No description provided for @categoryPdf.
  ///
  /// In en, this message translates to:
  /// **'PDF'**
  String get categoryPdf;

  /// No description provided for @categoryFiles.
  ///
  /// In en, this message translates to:
  /// **'Files'**
  String get categoryFiles;

  /// No description provided for @categoryConvert.
  ///
  /// In en, this message translates to:
  /// **'Convert'**
  String get categoryConvert;

  /// No description provided for @jobsPanelTitle.
  ///
  /// In en, this message translates to:
  /// **'Background Jobs'**
  String get jobsPanelTitle;

  /// No description provided for @jobsEmpty.
  ///
  /// In en, this message translates to:
  /// **'No active or recent jobs. Select any tool to begin.'**
  String get jobsEmpty;

  /// No description provided for @clearFinishedJobs.
  ///
  /// In en, this message translates to:
  /// **'Clear Finished'**
  String get clearFinishedJobs;

  /// No description provided for @cancelJobButton.
  ///
  /// In en, this message translates to:
  /// **'Cancel'**
  String get cancelJobButton;

  /// No description provided for @revealInFolderButton.
  ///
  /// In en, this message translates to:
  /// **'Reveal in Folder'**
  String get revealInFolderButton;

  /// No description provided for @openOutputButton.
  ///
  /// In en, this message translates to:
  /// **'Preview Output'**
  String get openOutputButton;

  /// No description provided for @pickFilesStep.
  ///
  /// In en, this message translates to:
  /// **'1. Pick Files'**
  String get pickFilesStep;

  /// No description provided for @addFilesButton.
  ///
  /// In en, this message translates to:
  /// **'Add Files'**
  String get addFilesButton;

  /// No description provided for @addFolderButton.
  ///
  /// In en, this message translates to:
  /// **'Add Folder'**
  String get addFolderButton;

  /// No description provided for @clearSelectionButton.
  ///
  /// In en, this message translates to:
  /// **'Clear All'**
  String get clearSelectionButton;

  /// No description provided for @optionsStep.
  ///
  /// In en, this message translates to:
  /// **'2. Options'**
  String get optionsStep;

  /// No description provided for @outputLocationLabel.
  ///
  /// In en, this message translates to:
  /// **'Save output to'**
  String get outputLocationLabel;

  /// No description provided for @outputNextToSource.
  ///
  /// In en, this message translates to:
  /// **'Next to original file (safe suffix)'**
  String get outputNextToSource;

  /// No description provided for @chooseOutputFolderButton.
  ///
  /// In en, this message translates to:
  /// **'Choose Folder...'**
  String get chooseOutputFolderButton;

  /// No description provided for @resetToDefaultButton.
  ///
  /// In en, this message translates to:
  /// **'Use Default'**
  String get resetToDefaultButton;

  /// No description provided for @runToolButton.
  ///
  /// In en, this message translates to:
  /// **'Run Tool'**
  String get runToolButton;

  /// No description provided for @runningStatus.
  ///
  /// In en, this message translates to:
  /// **'Running...'**
  String get runningStatus;

  /// No description provided for @resultsStep.
  ///
  /// In en, this message translates to:
  /// **'3. Results'**
  String get resultsStep;

  /// No description provided for @aiModelsButton.
  ///
  /// In en, this message translates to:
  /// **'AI Models (v2)'**
  String get aiModelsButton;

  /// No description provided for @aiModelsDialogTitle.
  ///
  /// In en, this message translates to:
  /// **'On-Demand AI Model Manager (v2 Preview)'**
  String get aiModelsDialogTitle;

  /// No description provided for @aiModelsDialogSubtitle.
  ///
  /// In en, this message translates to:
  /// **'Models are never bundled. Only permissive (MIT / Apache-2.0) ONNX models are permitted.'**
  String get aiModelsDialogSubtitle;

  /// No description provided for @themeTooltip.
  ///
  /// In en, this message translates to:
  /// **'Switch theme (System / Light / Dark)'**
  String get themeTooltip;

  /// No description provided for @backToHomeButton.
  ///
  /// In en, this message translates to:
  /// **'All Tools'**
  String get backToHomeButton;

  /// No description provided for @noToolsRegisteredYet.
  ///
  /// In en, this message translates to:
  /// **'Tools load dynamically from the Rust registry.'**
  String get noToolsRegisteredYet;

  /// No description provided for @officePreviewComingSoonTitle.
  ///
  /// In en, this message translates to:
  /// **'Preview coming soon'**
  String get officePreviewComingSoonTitle;

  /// No description provided for @officePreviewComingSoonBody.
  ///
  /// In en, this message translates to:
  /// **'Full offline rendering for Office documents (DOCX, XLSX, PPTX) is planned for a future release. You can open this file directly in your default system app below.'**
  String get officePreviewComingSoonBody;

  /// No description provided for @openInSystemAppButton.
  ///
  /// In en, this message translates to:
  /// **'Open in System App'**
  String get openInSystemAppButton;

  /// No description provided for @metadataPanelTitle.
  ///
  /// In en, this message translates to:
  /// **'File Metadata'**
  String get metadataPanelTitle;

  /// No description provided for @compatibleToolsTitle.
  ///
  /// In en, this message translates to:
  /// **'Quick Actions with This File'**
  String get compatibleToolsTitle;

  /// No description provided for @truncatedPreviewNotice.
  ///
  /// In en, this message translates to:
  /// **'Showing the first 256 KB so large files open instantly without high memory use.'**
  String get truncatedPreviewNotice;
}

class _AppLocalizationsDelegate
    extends LocalizationsDelegate<AppLocalizations> {
  const _AppLocalizationsDelegate();

  @override
  Future<AppLocalizations> load(Locale locale) {
    return SynchronousFuture<AppLocalizations>(lookupAppLocalizations(locale));
  }

  @override
  bool isSupported(Locale locale) =>
      <String>['en'].contains(locale.languageCode);

  @override
  bool shouldReload(_AppLocalizationsDelegate old) => false;
}

AppLocalizations lookupAppLocalizations(Locale locale) {
  // Lookup logic when only language code is specified.
  switch (locale.languageCode) {
    case 'en':
      return AppLocalizationsEn();
  }

  throw FlutterError(
    'AppLocalizations.delegate failed to load unsupported locale "$locale". This is likely '
    'an issue with the localizations generation tool. Please file an issue '
    'on GitHub with a reproducible sample app and the gen-l10n configuration '
    'that was used.',
  );
}
