// QuantaToneMap.fx — Complete ReShade Effect
// Generated helper functions by QuantaLang Compiler
// ReShade integration by hand (until texture sampling intrinsics ship)
//
// Usage: Drop this file into reshade-shaders/Shaders/
// Workflow: quantac reshade_tonemap.quanta --target hlsl -o reshade_tonemap.fx
//           Then wrap with ReShade boilerplate below.

// ============================================================
// ReShade Framework
// ============================================================
#include "ReShade.fxh"

// ============================================================
// Uniforms — adjustable in ReShade UI
// ============================================================
uniform float Exposure <
    ui_type = "slider";
    ui_min = -3.0; ui_max = 3.0; ui_step = 0.05;
    ui_label = "Exposure";
    ui_tooltip = "EV adjustment before tone mapping";
> = 0.0;

uniform float FilmicStrength <
    ui_type = "slider";
    ui_min = 0.0; ui_max = 1.0; ui_step = 0.01;
    ui_label = "Filmic Strength";
    ui_tooltip = "Blend between linear and ACES filmic curve";
> = 1.0;

// ============================================================
// QuantaLang-Generated Functions
// Source: reshade_tonemap.quanta
// Compiled: quantac reshade_tonemap.quanta --target hlsl
// ============================================================

float aces_tonemap(float x) {
    float a = 2.51;
    float b = 0.03;
    float c = 2.43;
    float d = 0.59;
    float e = 0.14;
    float num = x * (a * x + b);
    float den = x * (c * x + d) + e;
    return num / den;
}

float srgb_to_linear(float c) {
    if (c <= 0.04045) {
        return c / 12.92;
    } else {
        return pow((c + 0.055) / 1.055, 2.4);
    }
}

float linear_to_srgb(float c) {
    if (c <= 0.0031308) {
        return c * 12.92;
    } else {
        return 1.055 * pow(c, 0.416667) - 0.055;
    }
}

float process_channel(float c, float exposure) {
    float linear_val = srgb_to_linear(c);
    float exposed = linear_val * pow(2.0, exposure);
    float tonemapped = aces_tonemap(exposed);
    return linear_to_srgb(tonemapped);
}

// ============================================================
// ReShade Pixel Shader
// ============================================================
float4 PS_ToneMap(float4 pos : SV_Position, float2 texcoord : TEXCOORD) : SV_Target {
    float3 color = tex2D(ReShade::BackBuffer, texcoord).rgb;

    // Apply ACES tone mapping per channel
    float3 tonemapped;
    tonemapped.r = process_channel(color.r, Exposure);
    tonemapped.g = process_channel(color.g, Exposure);
    tonemapped.b = process_channel(color.b, Exposure);

    // Blend between original and tonemapped
    float3 result = lerp(color, tonemapped, FilmicStrength);

    return float4(result, 1.0);
}

// ============================================================
// Technique
// ============================================================
technique QuantaToneMap <
    ui_label = "Quanta Tone Map";
    ui_tooltip = "ACES filmic tone mapping — compiled from QuantaLang";
> {
    pass {
        VertexShader = PostProcessVS;
        PixelShader = PS_ToneMap;
    }
}
