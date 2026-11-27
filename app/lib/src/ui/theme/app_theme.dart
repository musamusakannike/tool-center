import 'package:material_ui/material_ui.dart';

class AppTheme {
  static const double radiusSm = 8.0;
  static const double radiusMd = 12.0;
  static const double radiusLg = 16.0;

  static ThemeData light() {
    final scheme = ColorScheme.fromSeed(
      seedColor: const Color(0xFF2563EB),
      brightness: Brightness.light,
      surface: const Color(0xFFF8FAFC),
    );
    return _baseTheme(scheme, isDark: false);
  }

  static ThemeData dark() {
    final scheme = ColorScheme.fromSeed(
      seedColor: const Color(0xFF60A5FA),
      brightness: Brightness.dark,
      surface: const Color(0xFF0F172A),
    );
    return _baseTheme(scheme, isDark: true);
  }

  static ThemeData _baseTheme(ColorScheme scheme, {required bool isDark}) {
    final cardColor = isDark
        ? const Color(0xFF1E293B)
        : const Color(0xFFFFFFFF);
    final borderColor = isDark
        ? const Color(0xFF334155)
        : const Color(0xFFE2E8F0);

    return ThemeData(
      useMaterial3: true,
      colorScheme: scheme,
      scaffoldBackgroundColor: scheme.surface,
      cardTheme: CardThemeData(
        color: cardColor,
        elevation: 0,
        margin: EdgeInsets.zero,
        shape: RoundedRectangleBorder(
          borderRadius: BorderRadius.circular(radiusMd),
          side: BorderSide(color: borderColor, width: 1),
        ),
      ),
      inputDecorationTheme: InputDecorationTheme(
        filled: true,
        fillColor: cardColor,
        isDense: true,
        contentPadding: const EdgeInsets.symmetric(
          horizontal: 14,
          vertical: 12,
        ),
        border: OutlineInputBorder(
          borderRadius: BorderRadius.circular(radiusSm),
          borderSide: BorderSide(color: borderColor),
        ),
        enabledBorder: OutlineInputBorder(
          borderRadius: BorderRadius.circular(radiusSm),
          borderSide: BorderSide(color: borderColor),
        ),
        focusedBorder: OutlineInputBorder(
          borderRadius: BorderRadius.circular(radiusSm),
          borderSide: BorderSide(color: scheme.primary, width: 1.6),
        ),
      ),
      filledButtonTheme: FilledButtonThemeData(
        style: FilledButton.styleFrom(
          padding: const EdgeInsets.symmetric(horizontal: 18, vertical: 14),
          shape: RoundedRectangleBorder(
            borderRadius: BorderRadius.circular(radiusSm),
          ),
        ),
      ),
      outlinedButtonTheme: OutlinedButtonThemeData(
        style: OutlinedButton.styleFrom(
          padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 12),
          shape: RoundedRectangleBorder(
            borderRadius: BorderRadius.circular(radiusSm),
          ),
          side: BorderSide(color: borderColor),
        ),
      ),
      dividerTheme: DividerThemeData(
        color: borderColor,
        thickness: 1,
        space: 1,
      ),
    );
  }
}

IconData resolveToolIcon(String iconName) {
  switch (iconName) {
    case 'crop':
      return Icons.crop_rounded;
    case 'resize':
      return Icons.photo_size_select_large_rounded;
    case 'rotate':
      return Icons.rotate_right_rounded;
    case 'convert_image':
      return Icons.transform_rounded;
    case 'compress':
      return Icons.compress_rounded;
    case 'watermark':
      return Icons.branding_watermark_rounded;
    case 'exif':
      return Icons.privacy_tip_outlined;
    case 'batch':
      return Icons.auto_awesome_motion_rounded;
    case 'pdf_merge':
      return Icons.call_merge_rounded;
    case 'pdf_split':
      return Icons.call_split_rounded;
    case 'pdf_extract':
      return Icons.filter_none_rounded;
    case 'pdf_reorder':
      return Icons.swap_vert_rounded;
    case 'pdf_rotate':
      return Icons.rotate_90_degrees_cw_rounded;
    case 'pdf_images':
      return Icons.collections_outlined;
    case 'images_pdf':
      return Icons.picture_as_pdf_outlined;
    case 'lock':
      return Icons.lock_outline_rounded;
    case 'unlock':
      return Icons.lock_open_rounded;
    case 'cut':
      return Icons.content_cut_rounded;
    case 'video':
      return Icons.movie_outlined;
    case 'audio':
      return Icons.audiotrack_rounded;
    case 'frames':
      return Icons.burst_mode_outlined;
    case 'gif':
      return Icons.gif_box_outlined;
    case 'archive':
      return Icons.folder_zip_outlined;
    case 'unarchive':
      return Icons.unarchive_outlined;
    case 'rename':
      return Icons.drive_file_rename_outline_rounded;
    case 'hash':
      return Icons.tag_rounded;
    case 'duplicate':
      return Icons.file_copy_outlined;
    case 'swap':
      return Icons.swap_horiz_rounded;
    default:
      return Icons.handyman_outlined;
  }
}
