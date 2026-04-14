use mvnx::{
    extract_xml_failures, filter_stack_trace, parse_module_start, parse_reactor_module,
    parse_test_results,
};

#[test]
fn test_parse_reactor_module() {
    // Valid module line
    assert_eq!(
        parse_reactor_module("1. com.example:module-name"),
        Some("com.example:module-name".to_string())
    );

    // With indentation and extra spaces
    assert_eq!(
        parse_reactor_module("  2. my-module  "),
        Some("my-module".to_string())
    );

    // Complex module name
    assert_eq!(
        parse_reactor_module("3. no.ks.fiks:vakthund-data"),
        Some("no.ks.fiks:vakthund-data".to_string())
    );

    // Non-matching line
    assert_eq!(parse_reactor_module("[INFO] something else"), None);

    // Empty string
    assert_eq!(parse_reactor_module(""), None);

    // No module number prefix
    assert_eq!(parse_reactor_module("com.example:module"), None);
}

#[test]
fn test_parse_module_start() {
    // Basic module name
    assert_eq!(
        parse_module_start("[INFO] Building component-test 1.0"),
        Some("component-test".to_string())
    );

    // Qualified module name
    assert_eq!(
        parse_module_start("[INFO] Building no.ks.fiks:vakthund-data 1.0-SNAPSHOT"),
        Some("no.ks.fiks:vakthund-data".to_string())
    );

    // With different spacing
    assert_eq!(
        parse_module_start("[INFO]  Building  mymodule  2.0"),
        Some("mymodule".to_string())
    );

    // Non-matching line
    assert_eq!(parse_module_start("[INFO] some other line"), None);

    // No [INFO] prefix
    assert_eq!(parse_module_start("Building component-test"), None);

    // Empty string
    assert_eq!(parse_module_start(""), None);
}

#[test]
fn test_parse_test_results() {
    // Basic test results with failures
    assert_eq!(
        parse_test_results("Tests run: 7, Failures: 1, Errors: 0, Skipped: 0"),
        Some((7, 1, 0, 0))
    );

    // All zeros
    assert_eq!(
        parse_test_results(
            "Tests run: 0, Failures: 0, Errors: 0, Skipped: 0, Time elapsed: 10.72 s -- in SomeTest"
        ),
        Some((0, 0, 0, 0))
    );

    // High numbers with errors
    assert_eq!(
        parse_test_results("Tests run: 42, Failures: 5, Errors: 3, Skipped: 2"),
        Some((42, 5, 3, 2))
    );

    // Non-matching line
    assert_eq!(parse_test_results("[INFO] nothing"), None);

    // Partial match (incomplete)
    assert_eq!(parse_test_results("Tests run: 5, Failures: 1"), None);

    // Empty string
    assert_eq!(parse_test_results(""), None);
}

#[test]
fn test_filter_stack_trace() {
    // Keep user code (no.ks.fiks package)
    let input = "at no.ks.fiks.vakthund.komponenttest.VarslingsKomponentTest.test()\nat java.lang.Thread.run()";
    let result = filter_stack_trace(input);
    assert!(result.contains("no.ks.fiks"));
    assert!(!result.contains("at java"));

    // Filter out framework packages
    let input = "at io.kotest.core.spec.style.scopes.FreeSpecContainerScope.invoke()\nat no.ks.fiks.myCode()\nat org.springframework.context.ApplicationContext.getBean()";
    let result = filter_stack_trace(input);
    assert!(result.contains("no.ks.fiks"));
    assert!(!result.contains("at io"));
    assert!(!result.contains("at org"));

    // Keep assertion messages (not at lines)
    let input = "java.lang.AssertionError: Expected 5 but got 3\nat java.lang.Thread.run()";
    let result = filter_stack_trace(input);
    assert!(result.contains("AssertionError: Expected 5 but got 3"));
    assert!(!result.contains("at java"));

    // Keep empty lines and non-at lines
    let input = "First line\n\nThird line";
    let result = filter_stack_trace(input);
    assert_eq!(result, "First line\n\nThird line");

    // Filter all mentioned packages
    let input = "at java.lang.Exception\nat kotlin.reflect.ClassClassifier\nat com.example.MyClass\nat feign.codec.Decoder\nat jdk.internal.misc.Unsafe\nat io.netty.channel.AbstractChannel\nat org.springframework.context.ApplicationContext";
    let result = filter_stack_trace(input);
    assert_eq!(result.trim(), ""); // All lines should be filtered
}

#[test]
fn test_extract_xml_failures_with_failure() {
    let xml = r#"<testcase name="testName" classname="DLQ" time="0.563">
    <failure message="&quot;jakarta.mail.internet.MimeMultipart@3b84fa44&quot; should include substring &quot;betaling.dlq&quot;" type="java.lang.AssertionError"><![CDATA[java.lang.AssertionError: "jakarta.mail.internet.MimeMultipart@3b84fa44" should include substring "betaling.dlq"
    at no.ks.fiks.vakthund.komponenttest.VarslingsKomponentTest$3$1.invokeSuspend(VarslingsKomponentTest.kt:163)
    at io.kotest.core.spec.style.scopes.FreeSpecContainerScope$invoke$2.invokeSuspend(FreeSpecContainerScope.kt:33)]]></failure>
</testcase>"#;

    let result = extract_xml_failures(xml);
    assert!(result.is_some());

    let output = result.unwrap();
    assert!(output.contains("should include substring"));
    assert!(output.contains("no.ks.fiks.vakthund.komponenttest")); // User code kept
    assert!(!output.contains("at io.kotest")); // Framework code filtered
}

#[test]
fn test_extract_xml_failures_with_error() {
    let xml = r#"<testcase name="testName" classname="MyTest" time="0.5">
    <error message="java.lang.NullPointerException" type="java.lang.NullPointerException"><![CDATA[java.lang.NullPointerException
    at no.ks.fiks.vakthund.MyCode.process(MyCode.kt:42)
    at java.lang.Thread.run()]]></error>
</testcase>"#;

    let result = extract_xml_failures(xml);
    assert!(result.is_some());

    let output = result.unwrap();
    assert!(output.contains("java.lang.NullPointerException"));
    assert!(output.contains("no.ks.fiks.vakthund")); // User code kept
    assert!(!output.contains("at java")); // Framework code filtered
}

#[test]
fn test_extract_xml_failures_no_failures() {
    let xml = r#"<testcase name="testName" classname="MyTest" time="0.5">
</testcase>"#;

    let result = extract_xml_failures(xml);
    assert_eq!(result, None);
}

#[test]
fn test_extract_xml_failures_multiple_failures() {
    let xml = r#"<testcase name="test1" classname="Test1" time="0.1">
    <failure message="First assertion failed" type="java.lang.AssertionError"><![CDATA[java.lang.AssertionError: First assertion failed
    at no.ks.fiks.test.method1(Test.kt:10)]]></failure>
</testcase>
<testcase name="test2" classname="Test2" time="0.2">
    <failure message="Second assertion failed" type="java.lang.AssertionError"><![CDATA[java.lang.AssertionError: Second assertion failed
    at no.ks.fiks.test.method2(Test.kt:20)]]></failure>
</testcase>"#;

    let result = extract_xml_failures(xml);
    assert!(result.is_some());

    let output = result.unwrap();
    assert!(output.contains("First assertion failed"));
    assert!(output.contains("Second assertion failed"));
    assert!(output.contains("method1"));
    assert!(output.contains("method2"));
}

#[test]
fn test_extract_xml_failures_special_characters() {
    let xml = r#"<failure message="&quot;value&quot; &lt; 5" type="AssertionError"><![CDATA[
    at no.ks.fiks.code.test()
]]></failure>"#;

    let result = extract_xml_failures(xml);
    assert!(result.is_some());

    let output = result.unwrap();
    // The message should contain the escaped HTML entities as-is (not unescaped by our regex)
    assert!(output.contains("&quot;value&quot;"));
    assert!(output.contains("&lt;"));
}
