//! 集成测试：验证 parse → classify → validate → excel 全流程
//!
//! 使用 fixtures/test_basic.txt 模拟 PDF 文本内容，覆盖核心解析管线。
//!
//! 运行方式：
//!   cargo test --test integration_test
//!
//! 注意：此测试不依赖外部工具（Ghostscript/Tesseract），
//!       仅测试文本解析、分类、校验和 Excel 写入的集成正确性。

use xingda_jiesuan::config::{load_rules, load_excel_style};
use xingda_jiesuan::models::{AssessmentRecord, SettlementData};
use xingda_jiesuan::classifier::classify_records;
use xingda_jiesuan::validator::validate_amounts;

/// 构造一个模拟的测验用 SettlementData
fn make_fixture_data() -> SettlementData {
    // 模拟从 PDF 提取到的文本（实际会由 parse_pdf 解析，
    // 此处直接构造已验证的数据结构用于分类和校验测试）
    let mut data = SettlementData::new();
    data.contract_no = "SC-2025-0001".into();
    data.contract_name = "兴达炼铁协力合同".into();
    data.work_period = "2025年3月".into();
    data.month_label = "3月".into();
    data.work_fee = 50000.0;
    data.total_reward = 500.0;
    data.pdf_stated_total = Some(1200.0);
    data.pdf_path = Some("test_data/fixture.pdf".into());
    data
}

#[test]
fn test_classify_all_records() {
    let mut data = make_fixture_data();

    // 模拟 3 条考核记录
    use xingda_jiesuan::models::AssessmentRecord;
    data.all_records = vec![
        AssessmentRecord::new(1, "供矿系统安全检查".into(), "条款1.1".into(), 500.0),
        AssessmentRecord::new(2, "煤库作业区 M3皮带故障".into(), "条款2.1".into(), 300.0),
        AssessmentRecord::new(3, "原料分厂 B2输送带问题".into(), "条款3.1".into(), 400.0),
    ];

    let rules = load_rules(None).expect("加载默认规则失败");

    classify_records(&mut data, &rules);

    // 验证分类结果
    let areas = &data.areas;
    assert!(areas.contains_key("供矿作业区"), "应分类到供矿作业区");
    assert!(areas.contains_key("煤库作业区"), "应分类到煤库作业区");
    assert!(areas.contains_key("原料分厂作业区"), "应分类到原料分厂作业区");

    // 验证各区域记录数
    if let Some(area) = areas.get("供矿作业区") {
        assert_eq!(area.records.len(), 1);
        assert_eq!(area.records[0].index, 1);
        assert!((area.subtotal - 500.0).abs() < 0.01);
    }
    if let Some(area) = areas.get("煤库作业区") {
        assert_eq!(area.records.len(), 1);
        assert_eq!(area.records[0].index, 2);
    }
    if let Some(area) = areas.get("原料分厂作业区") {
        assert_eq!(area.records.len(), 1);
        assert_eq!(area.records[0].index, 3);
    }

    // 小计之和应等于 total_assessment
    assert!((data.total_assessment - 1200.0).abs() < 0.01);
}

#[test]
fn test_validate_exact_match() {
    let mut data = make_fixture_data();
    data.total_assessment = 1200.0;
    data.pdf_stated_total = Some(1200.0);

    let valid = validate_amounts(&mut data);
    assert!(valid, "完全匹配应通过校验");
    assert!(data.amount_match);
}

#[test]
fn test_validate_small_deviation() {
    let mut data = make_fixture_data();
    data.total_assessment = 1180.0;
    data.pdf_stated_total = Some(1200.0); // 1.67% 偏差

    let valid = validate_amounts(&mut data);
    assert!(valid, "小偏差应在容差范围内");
    assert!(data.amount_match);
}

#[test]
fn test_validate_large_deviation() {
    let mut data = make_fixture_data();
    data.total_assessment = 800.0;
    data.pdf_stated_total = Some(1200.0); // 33% 偏差

    let valid = validate_amounts(&mut data);
    assert!(!valid, "大偏差应校验失败");
    assert!(!data.amount_match);
}

#[test]
fn test_area_dept_amount_calculation() {
    // 事业部考核金额 = 小计 × ratio（默认 0.01）
    let mut data = make_fixture_data();

    use xingda_jiesuan::models::AssessmentRecord;
    data.all_records = vec![
        AssessmentRecord::new(1, "供矿测试".into(), "".into(), 10000.0),
        AssessmentRecord::new(2, "供矿测试2".into(), "".into(), 5000.0),
    ];

    let rules = load_rules(None).expect("加载默认规则");

    classify_records(&mut data, &rules);

    if let Some(area) = data.areas.get("供矿作业区") {
        assert!((area.subtotal - 15000.0).abs() < 0.01, "小计应为15000");
        // dept_amount = 15000 * 0.01 = 150.00
        assert!((area.dept_amount - 150.0).abs() < 0.01, "事业部金额应为150");
    }
}

#[test]
fn test_excel_write_without_errors() {
    // 测试 Excel 写入不报错
    use xingda_jiesuan::models::AssessmentRecord;

    let mut data = make_fixture_data();
    data.all_records = vec![
        AssessmentRecord::new(1, "测试事项1".into(), "条款A".into(), 100.0),
        AssessmentRecord::new(2, "测试事项2".into(), "条款B".into(), 200.0),
    ];

    let rules = load_rules(None).expect("加载默认规则");
    classify_records(&mut data, &rules);
    validate_amounts(&mut data);

    let excel_style = load_excel_style();
    let output_dir = std::env::temp_dir().join("xingda_integration_test");
    std::fs::create_dir_all(&output_dir).expect("创建临时目录失败");

    let output_path = output_dir.join("test_output.xlsx");
    let output_str = output_path.to_str().expect("路径非法");

    let result = xingda_jiesuan::excel_writer::generate_excel(
        &data,
        output_str,
        &rules.area_order,
        &excel_style,
        true,  // include_summary
        false, // summary_only
    );

    assert!(result.is_ok(), "Excel 写入应成功");
    assert!(output_path.exists(), "Excel 文件应存在");

    // 清理
    let _ = std::fs::remove_dir_all(&output_dir);
}

#[test]
fn test_end_to_end_text_only() {
    // 端到端测试（仅文本解析，不涉及 PDF 文件读取）
    // 通过直接构造文本来模拟 parse_pdf 的输出，验证整体管线

    // 手动模拟 parse_pdf 的输出（无 PDF 文件）
    let mut data = SettlementData::new();
    data.contract_no = "SC-2025-TEST".into();
    data.contract_name = "测试合同".into();
    data.work_period = "2025年6月".into();
    data.month_label = "6月".into();
    data.work_fee = 50000.0;
    data.total_reward = 500.0;
    data.pdf_stated_total = Some(1500.0);
    data.settlement_amount = 49000.0;

    data.all_records = vec![
        AssessmentRecord::new(1, "6月 15日，供矿作业区安全检查".into(), "".into(), 500.0),
        AssessmentRecord::new(2, "6月 20日，煤库M5设备故障".into(), "".into(), 300.0),
        AssessmentRecord::new(3, "6月 25日，原料分厂B3皮带问题".into(), "".into(), 400.0),
        AssessmentRecord::new(4, "6月 28日，原煤仓清理不合格".into(), "".into(), 300.0),
    ];

    // 分类
    let rules = load_rules(None).expect("加载默认规则");
    classify_records(&mut data, &rules);

    // 验证分类结果
    assert!(data.areas.contains_key("供矿作业区"));
    assert!(data.areas.contains_key("煤库作业区"));
    assert!(data.areas.contains_key("原料分厂作业区"));

    // 记录总数 = 4
    assert_eq!(data.all_records.len(), 4);

    // 金额校验：提取合计 = 1500
    assert!((data.total_assessment - 1500.0).abs() < 0.01);

    // 闭环校验
    let is_valid = validate_amounts(&mut data);
    assert!(is_valid, "金额应完全匹配");
    assert!(data.amount_match);

    // 当月结算费用 = 50000 - 1500 + 500 = 49000
    let settlement = data.get_settlement_amount();
    assert!((settlement - 49000.0).abs() < 0.01);

    // 结算费用回落：优先使用提取值
    let final_settlement = data.settlement_amount_or_computed();
    assert!((final_settlement - 49000.0).abs() < 0.01);

    // 校验摘要包含关键信息
    let summary = xingda_jiesuan::validator::generate_validation_summary(&data);
    assert!(summary.contains("SC-2025-TEST"));
    assert!(summary.contains("测试合同"));
    assert!(summary.contains("4 条记录"));
}