import 'package:flutter_test/flutter_test.dart';
import 'package:tool_center/main.dart';

void main() {
  testWidgets('Tool Center shell renders title', (WidgetTester tester) async {
    await tester.pumpWidget(const ToolCenterApp());
    expect(find.text('Tool Center'), findsWidgets);
  });
}
