// PecoFence Paper: a Direct2D custom pixel shader with no inputs.
//
// A procedural sheet of matte paper in plate-local DIPs, so its texture travels with the
// fence and keeps its physical size at every DPI: broad mottling, sparse curved fibres,
// fine grain and rare inclusions form a height field whose relief is lit from the same
// light as Liquid Glass; the outline darkens slightly where the sheet ends. The wallpaper
// showing through is composited outside the shader, so this output can be cached and
// only re-renders on resize or a seed/colour change.
//
// Rebuild after editing: scripts/compile-shaders.sh (fxc /T ps_4_0).

cbuffer Constants : register(b0)
{
    float2 size;        // plate size, device px
    float radius;       // corner radius, device px (DWM rounds the window to it)
    float scale;        // device px per DIP
    float3 paper;       // base colour (linear 0..1)
    float tone;         // strength of the broad mottling
    float2 light;       // unit vector towards the light, screen space (y down)
    float fiber;        // fibre relief
    float grain;        // fine grain relief
    float relief;       // overall emboss strength
    float seed;         // per-fence variation
    float edge;         // darkening at the sheet's edge
    float speckle;      // inclusion density 0..1
};

float hash(float2 p)
{
    float3 q = frac(float3(p.xyx) * 0.1031 + seed * 0.0173);
    q += dot(q, q.yzx + 33.33);
    return frac((q.x + q.y) * q.z);
}

float noise(float2 p)
{
    float2 i = floor(p);
    float2 f = frac(p);
    float2 u = f * f * (3.0 - 2.0 * f);
    return lerp(lerp(hash(i), hash(i + float2(1.0, 0.0)), u.x),
                lerp(hash(i + float2(0.0, 1.0)), hash(i + float2(1.0, 1.0)), u.x), u.y);
}

float fbm(float2 p)
{
    float value = 0.0;
    float amplitude = 0.5;
    const float2x2 turn = float2x2(1.6, 1.2, -1.2, 1.6);
    [loop] for (int i = 0; i < 4; i++)
    {
        value += amplitude * noise(p);
        p = mul(turn, p);
        amplitude *= 0.5;
    }
    return value;
}

// Thin, curving strands with no dominant direction: noise squeezed along one axis in three
// orientations, after a strong low-frequency domain warp that bends each strand, then
// sharpened so only sparse ridges remain (two fixed directions read as woven linen).
float fibers(float2 p)
{
    float2 w = p + 14.0 * float2(noise(p * 0.025), noise(p * 0.025 + 7.3));
    float a = noise(mul(float2x2(0.94, 0.34, -0.34, 0.94), w) * float2(0.08, 1.2));
    float b = noise(mul(float2x2(0.50, -0.87, 0.87, 0.50), w) * float2(0.10, 1.0) + 13.1);
    float c = noise(mul(float2x2(-0.17, 0.98, -0.98, -0.17), w) * float2(0.09, 1.1) + 29.7);
    return pow(saturate(max(max(a, b), c)), 12.0);
}

float height(float2 p)
{
    return 0.5 * fbm(p * 0.045) + fiber * fibers(p) + grain * noise(p * 1.1);
}

float roundedDistance(float2 p)
{
    float2 q = abs(p - size * 0.5) - (size * 0.5 - radius);
    return length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - radius;
}

float4 main(float4 pos : SV_POSITION, float4 scene : SCENE_POSITION) : SV_Target
{
    float2 p = scene.xy / scale;

    // Emboss: the height field's slope, lit from the upper left. Flat paper stays at 1.
    // One loop body instead of three inlined copies keeps the bytecode small.
    float h = 0.0, hx = 0.0, hy = 0.0;
    [loop] for (int k = 0; k < 3; k++)
    {
        float2 offset = k == 1 ? float2(0.5, 0.0) : (k == 2 ? float2(0.0, 0.5) : float2(0.0, 0.0));
        float value = height(p + offset);
        h = k == 0 ? value : h;
        hx = k == 1 ? value : hx;
        hy = k == 2 ? value : hy;
    }
    float3 normal = normalize(float3((h - hx) * relief, (h - hy) * relief, 1.0));
    float3 toLight = normalize(float3(light, 0.9));
    float lit = 1.0 + (dot(normal, toLight) - toLight.z) * 1.6;

    float mottle = 1.0 + tone * (fbm(p * 0.011 + 3.7) - 0.5);
    float3 color = paper * mottle * lit;

    // Rare small inclusions, a couple of DIPs across.
    float2 cell = floor(p * 0.6);
    float spot = step(1.0 - speckle * 0.004, hash(cell + 71.0));
    float shape = saturate(1.0 - length(frac(p * 0.6) - 0.5) * 2.6);
    color *= 1.0 - spot * shape * 0.22;

    // The sheet's edge sits a little darker, within ~2 DIP of the outline.
    float depth = max(-roundedDistance(scene.xy), 0.0) / scale;
    color *= 1.0 - edge * exp(-depth * 0.7);
    return float4(saturate(color), 1.0);
}
