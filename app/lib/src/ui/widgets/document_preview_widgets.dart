import 'dart:convert';

import 'package:flutter/services.dart';
import 'package:highlight/highlight.dart' show highlight, Node;
import 'package:markdown/markdown.dart' as md;
import 'package:material_ui/material_ui.dart';

import '../../models/preview_models.dart';

/// Syntax-highlighted code & plain-text viewer with line numbers and light/dark palettes.
class SyntaxCodePreview extends StatelessWidget {
  final String source;
  final String? language;
  final bool truncated;
  final String truncatedNotice;

  const SyntaxCodePreview({
    super.key,
    required this.source,
    this.language,
    this.truncated = false,
    required this.truncatedNotice,
  });

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final isDark = theme.brightness == Brightness.dark;
    final palette = isDark ? _darkSyntaxPalette : _lightSyntaxPalette;
    final lineCount = '\n'.allMatches(source).length + 1;
    final lineNums = List.generate(
      lineCount.clamp(1, 5000),
      (i) => '${i + 1}',
    ).join('\n');

    List<TextSpan> spans;
    if (language != null && source.length <= 200000) {
      try {
        final result = highlight.parse(source, language: language);
        spans = _convertHighlightNodes(result.nodes ?? const [], palette);
      } catch (_) {
        spans = [TextSpan(text: source)];
      }
    } else {
      spans = [TextSpan(text: source)];
    }

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        if (truncated)
          Container(
            padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 8),
            color: theme.colorScheme.secondaryContainer,
            child: Row(
              children: [
                Icon(
                  Icons.bolt_rounded,
                  size: 16,
                  color: theme.colorScheme.onSecondaryContainer,
                ),
                const SizedBox(width: 8),
                Expanded(
                  child: Text(
                    truncatedNotice,
                    style: theme.textTheme.bodySmall?.copyWith(
                      color: theme.colorScheme.onSecondaryContainer,
                    ),
                  ),
                ),
              ],
            ),
          ),
        Align(
          alignment: Alignment.centerRight,
          child: Padding(
            padding: const EdgeInsets.fromLTRB(12, 8, 12, 0),
            child: TextButton.icon(
              onPressed: () {
                Clipboard.setData(ClipboardData(text: source));
                ScaffoldMessenger.of(context).showSnackBar(
                  const SnackBar(
                    content: Text('Copied preview text to clipboard'),
                    duration: Duration(seconds: 1),
                  ),
                );
              },
              icon: const Icon(Icons.copy_rounded, size: 16),
              label: const Text('Copy'),
            ),
          ),
        ),
        Expanded(
          child: SingleChildScrollView(
            padding: const EdgeInsets.all(16),
            child: SingleChildScrollView(
              scrollDirection: Axis.horizontal,
              child: Row(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    lineNums,
                    textAlign: TextAlign.right,
                    style: TextStyle(
                      fontFamily: 'monospace',
                      fontSize: 13,
                      height: 1.5,
                      color: theme.colorScheme.onSurfaceVariant.withValues(
                        alpha: 0.55,
                      ),
                    ),
                  ),
                  const SizedBox(width: 16),
                  SelectableText.rich(
                    TextSpan(
                      style: TextStyle(
                        fontFamily: 'monospace',
                        fontSize: 13,
                        height: 1.5,
                        color: palette['root'],
                      ),
                      children: spans,
                    ),
                  ),
                ],
              ),
            ),
          ),
        ),
      ],
    );
  }

  List<TextSpan> _convertHighlightNodes(
    List<Node> nodes,
    Map<String, Color> palette,
  ) {
    final out = <TextSpan>[];
    for (final node in nodes) {
      if (node.value != null) {
        out.add(
          TextSpan(
            text: node.value,
            style: node.className != null
                ? TextStyle(color: palette[node.className!])
                : null,
          ),
        );
      } else if (node.children != null) {
        out.add(
          TextSpan(
            style: node.className != null
                ? TextStyle(color: palette[node.className!])
                : null,
            children: _convertHighlightNodes(node.children!, palette),
          ),
        );
      }
    }
    return out;
  }
}

const Map<String, Color> _lightSyntaxPalette = {
  'root': Color(0xFF1E293B),
  'keyword': Color(0xFF7C3AED),
  'built_in': Color(0xFF0284C7),
  'type': Color(0xFF0284C7),
  'literal': Color(0xFFD97706),
  'number': Color(0xFFD97706),
  'string': Color(0xFF15803D),
  'comment': Color(0xFF64748B),
  'title': Color(0xFF1D4ED8),
  'attr': Color(0xFFB45309),
  'meta': Color(0xFF6D28D9),
};

const Map<String, Color> _darkSyntaxPalette = {
  'root': Color(0xFFE2E8F0),
  'keyword': Color(0xFFC084FC),
  'built_in': Color(0xFF38BDF8),
  'type': Color(0xFF38BDF8),
  'literal': Color(0xFFFBBF24),
  'number': Color(0xFFFBBF24),
  'string': Color(0xFF4ADE80),
  'comment': Color(0xFF94A3B8),
  'title': Color(0xFF60A5FA),
  'attr': Color(0xFFFCD34D),
  'meta': Color(0xFFA78BFA),
};

/// Native Markdown AST renderer + raw source toggle.
class MarkdownPreviewPane extends StatefulWidget {
  final TextPreviewModel preview;
  final String truncatedNotice;

  const MarkdownPreviewPane({
    super.key,
    required this.preview,
    required this.truncatedNotice,
  });

  @override
  State<MarkdownPreviewPane> createState() => _MarkdownPreviewPaneState();
}

class _MarkdownPreviewPaneState extends State<MarkdownPreviewPane> {
  bool _showSource = false;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final nodes = md.Document(
      extensionSet: md.ExtensionSet.gitHubWeb,
    ).parseLines(const LineSplitter().convert(widget.preview.text));

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(16, 10, 16, 6),
          child: Row(
            children: [
              SegmentedButton<bool>(
                segments: const [
                  ButtonSegment(
                    value: false,
                    icon: Icon(Icons.article_outlined, size: 16),
                    label: Text('Rendered'),
                  ),
                  ButtonSegment(
                    value: true,
                    icon: Icon(Icons.code_rounded, size: 16),
                    label: Text('Markdown Source'),
                  ),
                ],
                selected: {_showSource},
                onSelectionChanged: (s) =>
                    setState(() => _showSource = s.first),
              ),
            ],
          ),
        ),
        const Divider(),
        Expanded(
          child: _showSource
              ? SyntaxCodePreview(
                  source: widget.preview.text,
                  language: 'markdown',
                  truncated: widget.preview.truncated,
                  truncatedNotice: widget.truncatedNotice,
                )
              : SingleChildScrollView(
                  padding: const EdgeInsets.all(24),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      for (final node in nodes)
                        Padding(
                          padding: const EdgeInsets.only(bottom: 12),
                          child: _renderMdBlock(context, theme, node),
                        ),
                    ],
                  ),
                ),
        ),
      ],
    );
  }

  Widget _renderMdBlock(BuildContext context, ThemeData theme, md.Node node) {
    if (node is md.Element) {
      final text = node.textContent;
      switch (node.tag) {
        case 'h1':
          return SelectableText(
            text,
            style: theme.textTheme.headlineMedium?.copyWith(
              fontWeight: FontWeight.w800,
            ),
          );
        case 'h2':
          return SelectableText(
            text,
            style: theme.textTheme.headlineSmall?.copyWith(
              fontWeight: FontWeight.w700,
            ),
          );
        case 'h3':
          return SelectableText(
            text,
            style: theme.textTheme.titleLarge?.copyWith(
              fontWeight: FontWeight.w700,
            ),
          );
        case 'pre':
          return Container(
            width: double.infinity,
            padding: const EdgeInsets.all(14),
            decoration: BoxDecoration(
              color: theme.colorScheme.surfaceContainerHighest,
              borderRadius: BorderRadius.circular(8),
            ),
            child: SelectableText(
              text,
              style: const TextStyle(fontFamily: 'monospace', fontSize: 13),
            ),
          );
        case 'blockquote':
          return Container(
            padding: const EdgeInsets.only(left: 14, top: 4, bottom: 4),
            decoration: BoxDecoration(
              border: Border(
                left: BorderSide(color: theme.colorScheme.primary, width: 3),
              ),
            ),
            child: SelectableText(
              text,
              style: theme.textTheme.bodyMedium?.copyWith(
                fontStyle: FontStyle.italic,
              ),
            ),
          );
        case 'ul':
        case 'ol':
          final items = node.children?.whereType<md.Element>().toList() ?? [];
          return Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              for (int i = 0; i < items.length; i++)
                Padding(
                  padding: const EdgeInsets.only(bottom: 4),
                  child: SelectableText(
                    node.tag == 'ol'
                        ? '${i + 1}. ${items[i].textContent}'
                        : '• ${items[i].textContent}',
                    style: theme.textTheme.bodyLarge,
                  ),
                ),
            ],
          );
        default:
          return SelectableText(text, style: theme.textTheme.bodyLarge);
      }
    }
    return SelectableText(node.textContent, style: theme.textTheme.bodyLarge);
  }
}

/// Tabular CSV/TSV viewer + raw source toggle.
class CsvTablePreviewPane extends StatefulWidget {
  final TextPreviewModel preview;
  final String truncatedNotice;

  const CsvTablePreviewPane({
    super.key,
    required this.preview,
    required this.truncatedNotice,
  });

  @override
  State<CsvTablePreviewPane> createState() => _CsvTablePreviewPaneState();
}

class _CsvTablePreviewPaneState extends State<CsvTablePreviewPane> {
  bool _showRaw = false;

  @override
  Widget build(BuildContext context) {
    final headers = widget.preview.csvHeaders ?? const [];
    final rows = widget.preview.csvRows ?? const [];

    if (headers.isEmpty) {
      return SyntaxCodePreview(
        source: widget.preview.text,
        truncated: widget.preview.truncated,
        truncatedNotice: widget.truncatedNotice,
      );
    }

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(16, 10, 16, 6),
          child: Row(
            children: [
              SegmentedButton<bool>(
                segments: const [
                  ButtonSegment(
                    value: false,
                    icon: Icon(Icons.table_chart_outlined, size: 16),
                    label: Text('Table'),
                  ),
                  ButtonSegment(
                    value: true,
                    icon: Icon(Icons.notes_rounded, size: 16),
                    label: Text('Raw CSV'),
                  ),
                ],
                selected: {_showRaw},
                onSelectionChanged: (s) => setState(() => _showRaw = s.first),
              ),
              const SizedBox(width: 12),
              Text(
                'Showing ${rows.length} rows × ${headers.length} columns',
                style: Theme.of(context).textTheme.bodySmall,
              ),
            ],
          ),
        ),
        const Divider(),
        Expanded(
          child: _showRaw
              ? SyntaxCodePreview(
                  source: widget.preview.text,
                  truncated: widget.preview.truncated,
                  truncatedNotice: widget.truncatedNotice,
                )
              : SingleChildScrollView(
                  padding: const EdgeInsets.all(12),
                  child: SingleChildScrollView(
                    scrollDirection: Axis.horizontal,
                    child: DataTable(
                      columns: [
                        const DataColumn(label: Text('#')),
                        for (final h in headers)
                          DataColumn(
                            label: Text(
                              h,
                              style: const TextStyle(
                                fontWeight: FontWeight.w700,
                              ),
                            ),
                          ),
                      ],
                      rows: [
                        for (int r = 0; r < rows.length; r++)
                          DataRow(
                            cells: [
                              DataCell(Text('${r + 1}')),
                              for (int c = 0; c < headers.length; c++)
                                DataCell(
                                  ConstrainedBox(
                                    constraints: const BoxConstraints(
                                      maxWidth: 280,
                                    ),
                                    child: Text(
                                      c < rows[r].length ? rows[r][c] : '',
                                      overflow: TextOverflow.ellipsis,
                                    ),
                                  ),
                                ),
                            ],
                          ),
                      ],
                    ),
                  ),
                ),
        ),
      ],
    );
  }
}

/// Collapsible JSON Tree + formatted syntax-highlighted JSON toggle.
class JsonPreviewPane extends StatefulWidget {
  final TextPreviewModel preview;
  final String truncatedNotice;

  const JsonPreviewPane({
    super.key,
    required this.preview,
    required this.truncatedNotice,
  });

  @override
  State<JsonPreviewPane> createState() => _JsonPreviewPaneState();
}

class _JsonPreviewPaneState extends State<JsonPreviewPane> {
  bool _showRaw = false;

  @override
  Widget build(BuildContext context) {
    dynamic parsed;
    if (!widget.preview.truncated) {
      try {
        parsed = jsonDecode(widget.preview.text);
      } catch (_) {}
    }

    if (parsed == null) {
      return SyntaxCodePreview(
        source: widget.preview.text,
        language: 'json',
        truncated: widget.preview.truncated,
        truncatedNotice: widget.truncatedNotice,
      );
    }

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(16, 10, 16, 6),
          child: Row(
            children: [
              SegmentedButton<bool>(
                segments: const [
                  ButtonSegment(
                    value: false,
                    icon: Icon(Icons.account_tree_outlined, size: 16),
                    label: Text('Tree'),
                  ),
                  ButtonSegment(
                    value: true,
                    icon: Icon(Icons.data_object_rounded, size: 16),
                    label: Text('Formatted JSON'),
                  ),
                ],
                selected: {_showRaw},
                onSelectionChanged: (s) => setState(() => _showRaw = s.first),
              ),
            ],
          ),
        ),
        const Divider(),
        Expanded(
          child: _showRaw
              ? SyntaxCodePreview(
                  source: widget.preview.text,
                  language: 'json',
                  truncated: widget.preview.truncated,
                  truncatedNotice: widget.truncatedNotice,
                )
              : ListView(
                  padding: const EdgeInsets.all(16),
                  children: [
                    _JsonTreeNode(nodeKey: 'root', value: parsed, depth: 0),
                  ],
                ),
        ),
      ],
    );
  }
}

class _JsonTreeNode extends StatelessWidget {
  final String nodeKey;
  final dynamic value;
  final int depth;

  const _JsonTreeNode({
    required this.nodeKey,
    required this.value,
    required this.depth,
  });

  @override
  Widget build(BuildContext context) {
    if (value is Map<String, dynamic>) {
      final entries = value.entries.toList();
      return ExpansionTile(
        initiallyExpanded: depth < 2,
        dense: true,
        title: Text(
          '$nodeKey  {${entries.length}}',
          style: const TextStyle(
            fontFamily: 'monospace',
            fontWeight: FontWeight.w600,
          ),
        ),
        childrenPadding: const EdgeInsets.only(left: 16),
        children: [
          for (final e in entries.take(250))
            _JsonTreeNode(nodeKey: e.key, value: e.value, depth: depth + 1),
        ],
      );
    }
    if (value is List<dynamic>) {
      return ExpansionTile(
        initiallyExpanded: depth < 1,
        dense: true,
        title: Text(
          '$nodeKey  [${value.length}]',
          style: const TextStyle(
            fontFamily: 'monospace',
            fontWeight: FontWeight.w600,
          ),
        ),
        childrenPadding: const EdgeInsets.only(left: 16),
        children: [
          for (int i = 0; i < value.length && i < 250; i++)
            _JsonTreeNode(nodeKey: '[$i]', value: value[i], depth: depth + 1),
        ],
      );
    }

    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 4),
      child: SelectableText(
        '$nodeKey: ${jsonEncode(value)}',
        style: const TextStyle(fontFamily: 'monospace', fontSize: 13),
      ),
    );
  }
}
