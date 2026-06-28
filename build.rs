// PiPL（プラグイン定義）をビルド時生成する。
// custom_comp_ui example の build.rs を踏襲し、名前・カテゴリ・matchName のみ変更。
use pipl::*;

const PF_PLUG_IN_VERSION: u16 = 13;
const PF_PLUG_IN_SUBVERS: u16 = 28;

#[rustfmt::skip]
fn main() {
    pipl::plugin_build(vec![
        Property::Kind(PIPLType::AEEffect),
        Property::Name("Yato Gradient"),
        Property::Category("Yato"),

        #[cfg(target_os = "windows")]
        Property::CodeWin64X86("EffectMain"),
        #[cfg(target_os = "macos")]
        Property::CodeMacIntel64("EffectMain"),
        #[cfg(target_os = "macos")]
        Property::CodeMacARM64("EffectMain"),

        Property::AE_PiPL_Version { major: 2, minor: 0 },
        Property::AE_Effect_Spec_Version { major: PF_PLUG_IN_VERSION, minor: PF_PLUG_IN_SUBVERS },
        Property::AE_Effect_Version {
            version: 1,
            subversion: 0,
            bugversion: 0,
            stage: Stage::Develop,
            build: 1,
        },
        // custom_comp_ui example と同値。CustomUI 効果では 3 が必要らしく、
        // 0 にすると AE が「必須 PiPL プロパティが無い」とエラーを出してクラッシュする。
        Property::AE_Effect_Info_Flags(3),
        Property::AE_Effect_Global_OutFlags(
            OutFlags::CustomUI |
            OutFlags::PixIndependent |
            OutFlags::UseOutputExtent |
            OutFlags::DeepColorAware
        ),
        Property::AE_Effect_Global_OutFlags_2(
            OutFlags2::SupportsThreadedRendering |
            OutFlags2::SupportsGetFlattenedSequenceData
        ),
        Property::AE_Effect_Match_Name("YATO Gradient v1"), // 一意であること
        Property::AE_Reserved_Info(0),
        Property::AE_Effect_Support_URL("https://github.com/hakuseiyato"),
    ])
}
