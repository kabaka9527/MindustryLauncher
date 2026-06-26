using System.Text.Json;
using System.Text.Json.Serialization;
using MindustryLauncher.Models;

namespace MindustryLauncher.Services;

public static class JsonSettings
{
    public static readonly JsonSerializerOptions Options = CreateOptions();

    private static JsonSerializerOptions CreateOptions()
    {
        var options = new JsonSerializerOptions
        {
            PropertyNamingPolicy = JsonNamingPolicy.CamelCase,
            PropertyNameCaseInsensitive = true,
            WriteIndented = true
        };
        options.Converters.Add(new GameChannelConverter());
        options.Converters.Add(new RuntimeSourceConverter());
        options.Converters.Add(new ThemePreferenceConverter());
        return options;
    }
}

public sealed class GameChannelConverter : JsonConverter<GameChannel>
{
    public override GameChannel Read(ref Utf8JsonReader reader, Type typeToConvert, JsonSerializerOptions options)
    {
        return reader.TokenType == JsonTokenType.String
            && LauncherModelExtensions.TryParseGameChannel(reader.GetString(), out var channel)
                ? channel
                : GameChannel.Mindustry;
    }

    public override void Write(Utf8JsonWriter writer, GameChannel value, JsonSerializerOptions options)
    {
        writer.WriteStringValue(value.ToWireValue());
    }
}

public sealed class RuntimeSourceConverter : JsonConverter<RuntimeSource>
{
    public override RuntimeSource Read(ref Utf8JsonReader reader, Type typeToConvert, JsonSerializerOptions options)
    {
        return reader.TokenType == JsonTokenType.String
            && LauncherModelExtensions.TryParseRuntimeSource(reader.GetString(), out var source)
                ? source
                : RuntimeSource.Unknown;
    }

    public override void Write(Utf8JsonWriter writer, RuntimeSource value, JsonSerializerOptions options)
    {
        writer.WriteStringValue(value.ToWireValue());
    }
}

public sealed class ThemePreferenceConverter : JsonConverter<ThemePreference>
{
    public override ThemePreference Read(ref Utf8JsonReader reader, Type typeToConvert, JsonSerializerOptions options)
    {
        if (reader.TokenType != JsonTokenType.String)
        {
            return ThemePreference.System;
        }

        return reader.GetString() switch
        {
            "light" => ThemePreference.Light,
            "dark" => ThemePreference.Dark,
            _ => ThemePreference.System
        };
    }

    public override void Write(Utf8JsonWriter writer, ThemePreference value, JsonSerializerOptions options)
    {
        writer.WriteStringValue(value switch
        {
            ThemePreference.Light => "light",
            ThemePreference.Dark => "dark",
            _ => "system"
        });
    }
}
