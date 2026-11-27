import 'package:flutter/services.dart';
import 'package:material_ui/material_ui.dart';

import '../../../l10n/app_localizations.dart';
import '../../models/tool_models.dart';
import '../theme/app_theme.dart';

Future<ToolDescriptorModel?> showCommandPaletteDialog({
  required BuildContext context,
  required List<ToolDescriptorModel> tools,
}) {
  return showDialog<ToolDescriptorModel>(
    context: context,
    builder: (ctx) => _CommandPaletteDialog(tools: tools),
  );
}

class _CommandPaletteDialog extends StatefulWidget {
  final List<ToolDescriptorModel> tools;

  const _CommandPaletteDialog({required this.tools});

  @override
  State<_CommandPaletteDialog> createState() => _CommandPaletteDialogState();
}

class _CommandPaletteDialogState extends State<_CommandPaletteDialog> {
  final TextEditingController _controller = TextEditingController();
  int _highlightedIndex = 0;

  List<ToolDescriptorModel> get _matches {
    final q = _controller.text;
    return widget.tools.where((t) => t.matchesQuery(q)).toList();
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context);
    final matches = _matches;
    final safeIdx = matches.isEmpty
        ? 0
        : _highlightedIndex.clamp(0, matches.length - 1);

    return Dialog(
      insetPadding: const EdgeInsets.symmetric(horizontal: 24, vertical: 64),
      shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(14)),
      child: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: 580, maxHeight: 460),
        child: Shortcuts(
          shortcuts: const <ShortcutActivator, Intent>{
            SingleActivator(LogicalKeyboardKey.arrowDown):
                _MoveSelectionIntent(1),
            SingleActivator(LogicalKeyboardKey.arrowUp):
                _MoveSelectionIntent(-1),
            SingleActivator(LogicalKeyboardKey.enter): _ActivateSelectionIntent(),
          },
          child: Actions(
            actions: <Type, Action<Intent>>{
              _MoveSelectionIntent: CallbackAction<_MoveSelectionIntent>(
                onInvoke: (intent) {
                  if (matches.isEmpty) return null;
                  setState(() {
                    _highlightedIndex =
                        (safeIdx + intent.delta).clamp(0, matches.length - 1);
                  });
                  return null;
                },
              ),
              _ActivateSelectionIntent:
                  CallbackAction<_ActivateSelectionIntent>(
                    onInvoke: (_) {
                      if (matches.isNotEmpty) {
                        Navigator.of(context).pop(matches[safeIdx]);
                      }
                      return null;
                    },
                  ),
            },
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                Padding(
                  padding: const EdgeInsets.all(14),
                  child: TextField(
                    controller: _controller,
                    autofocus: true,
                    decoration: InputDecoration(
                      prefixIcon: const Icon(Icons.search_rounded),
                      hintText: l10n.commandPalettePlaceholder,
                      suffixIcon: const Padding(
                        padding: EdgeInsets.only(right: 10),
                        child: Chip(
                          label: Text('ESC', style: TextStyle(fontSize: 11)),
                          visualDensity: VisualDensity.compact,
                        ),
                      ),
                    ),
                    onChanged: (_) => setState(() => _highlightedIndex = 0),
                  ),
                ),
                const Divider(),
                Flexible(
                  child: matches.isEmpty
                      ? Padding(
                          padding: const EdgeInsets.all(32),
                          child: Text(l10n.commandPaletteEmpty),
                        )
                      : ListView.builder(
                          shrinkWrap: true,
                          itemCount: matches.length,
                          itemBuilder: (context, index) {
                            final tool = matches[index];
                            final selected = index == safeIdx;
                            return ListTile(
                              selected: selected,
                              selectedTileColor: Theme.of(context)
                                  .colorScheme
                                  .primaryContainer
                                  .withValues(alpha: 0.45),
                              leading: Icon(resolveToolIcon(tool.icon)),
                              title: Text(
                                tool.name,
                                style: const TextStyle(
                                  fontWeight: FontWeight.w600,
                                ),
                              ),
                              subtitle: Text(
                                tool.description,
                                maxLines: 1,
                                overflow: TextOverflow.ellipsis,
                              ),
                              trailing: Text(
                                tool.category.name.toUpperCase(),
                                style: Theme.of(context).textTheme.labelSmall,
                              ),
                              onTap: () => Navigator.of(context).pop(tool),
                            );
                          },
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

class _MoveSelectionIntent extends Intent {
  final int delta;
  const _MoveSelectionIntent(this.delta);
}

class _ActivateSelectionIntent extends Intent {
  const _ActivateSelectionIntent();
}
