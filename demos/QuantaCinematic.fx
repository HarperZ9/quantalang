// QuantaCinematic.fx — Complete ReShade Cinematic Post-Processing
// All math functions compiled from QuantaLang source (cinematic.quanta)
// ReShade pixel shader integration hand-finished
//
// Drop into: reshade-shaders/Shaders/

#include "ReShade.fxh"

// ============================================================
// Uniforms — adjustable in ReShade UI
// ============================================================
uniform float VignetteStrength <
    ui_type = "slider";
    ui_min = 0.0; ui_max = 1.0; ui_step = 0.01;
    ui_label = "Vignette Strength";
> = 0.4;

uniform float VignetteSoftness <
    ui_type = "slider";
    ui_min = 0.1; ui_max = 1.0; ui_step = 0.01;
    ui_label = "Vignette Softness";
> = 0.6;

uniform float GrainAmount <
    ui_type = "slider";
    ui_min = 0.0; ui_max = 0.3; ui_step = 0.005;
    ui_label = "Film Grain";
> = 0.05;

uniform float Lift <
    ui_type = "slider";
    ui_min = 0.0; ui_max = 0.1; ui_step = 0.005;
    ui_label = "Lift (Shadow Brightness)";
> = 0.02;

uniform float Gamma <
    ui_type = "slider";
    ui_min = 0.5; ui_max = 1.5; ui_step = 0.01;
    ui_label = "Gamma (Midtone Curve)";
> = 0.95;

uniform float Gain <
    ui_type = "slider";
    ui_min = 0.5; ui_max = 2.0; ui_step = 0.01;
    ui_label = "Gain (Highlight Intensity)";
> = 1.1;

uniform float Timer < source = "timer"; >;

// ============================================================
// QuantaLang-Generated Functions
// Source: cinematic.quanta
// ============================================================

float hash(float n) {
    float x = sin(n) * 43758.5453;
    return frac(x);
}

float vignette(float uv_x, float uv_y, float strength, float softness) {
    float dx = uv_x - 0.5;
    float dy = uv_y - 0.5;
    float dist = sqrt(dx * dx + dy * dy);
    float vig = smoothstep(0.5, 0.5 * softness, dist);
    return 1.0 - strength * (1.0 - vig);
}

float film_grain(float uv_x, float uv_y, float seed, float amount) {
    float noise = hash(uv_x * 12.9898 + uv_y * 78.233 + seed);
    float grain = (noise - 0.5) * 2.0 * amount;
    return grain;
}

float lift_gamma_gain(float c, float lift_val, float gamma_val, float gain_val) {
    float lifted = c * (1.0 - lift_val) + lift_val;
    float gained = lifted * gain_val;
    float graded = pow(gained, 1.0 / gamma_val);
    return clamp(graded, 0.0, 1.0);
}

float srgb_to_linear(float c) {
    if (c <= 0.04045)
        return c / 12.92;
    else
        return pow((c + 0.055) / 1.055, 2.4);
}

float linear_to_srgb(float c) {
    if (c <= 0.0031308)
        return c * 12.92;
    else
        return 1.055 * pow(c, 0.416667) - 0.055;
}

// ============================================================
// ReShade Pixel Shader
// ============================================================
float4 PS_Cinematic(float4 pos : SV_Position, float2 texcoord : TEXCOORD) : SV_Target {
    float3 color = tex2D(ReShade::BackBuffer, texcoord).rgb;

    // Work in linear space
    float3 linear_color;
    linear_color.r = srgb_to_linear(color.r);
    linear_color.g = srgb_to_linear(color.g);
    linear_color.b = srgb_to_linear(color.b);

    // Color grading: lift-gamma-gain
    linear_color.r = lift_gamma_gain(linear_color.r, Lift, Gamma, Gain);
    linear_color.g = lift_gamma_gain(linear_color.g, Lift, Gamma, Gain);
    linear_color.b = lift_gamma_gain(linear_color.b, Lift, Gamma, Gain);

    // Vignette
    float vig = vignette(texcoord.x, texcoord.y, VignetteStrength, VignetteSoftness);
    linear_color *= vig;

    // Film grain (time-varying)
    float seed = Timer * 0.001;
    float grain = film_grain(texcoord.x, texcoord.y, seed, GrainAmount);
    linear_color += grain * linear_color;

    // Back to sRGB
    float3 result;
    result.r = linear_to_srgb(linear_color.r);
    result.g = linear_to_srgb(linear_color.g);
    result.b = linear_to_srgb(linear_color.b);

    return float4(result, 1.0);
}

// ============================================================
// Technique
// ============================================================
technique QuantaCinematic <
    ui_label = "Quanta Cinematic";
    ui_tooltip = "Vignette + Film Grain + Lift/Gamma/Gain\nCompiled from QuantaLang";
> {
    pass {
        VertexShader = PostProcessVS;
        PixelShader = PS_Cinematic;
    }
}
