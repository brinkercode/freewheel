# freewheel data model

## Config

| field | type |
| --- | --- |
| version | u32 (1) |
| settings | Settings |
| layers | Vec<Layer> |
| base_layer | LayerId |
| jog | JogSettings |
| overrides | OverrideSettings |

Refers to: Settings, Layer, JogSettings, OverrideSettings

## Settings

| field | type |
| --- | --- |
| enabled | bool |
| start_at_login | bool (default false) |
| resolve_process_name | String (Resolve.exe \| Resolve) |
| low_battery_threshold | u8 (15) |

Refers to: Config

## Layer

| field | type |
| --- | --- |
| id | LayerId |
| name | String |
| bindings | Map<Key, Action> |

Refers to: Config, Action

## Action

| field | type |
| --- | --- |
| Hotkey{combo | KeyCombo} |
| Held{combo | KeyCombo} |
| Latched{combo | KeyCombo} |
| Scroll{amount | i32} (P1) |
| LaunchApp{path | String, args: Vec<String>, enabled: bool} |
| LayerPush{target | LayerId} |
| SetJogMode{mode | JogMode} |
| OverrideToggle{mode | OverrideMode} |

Refers to: KeyCombo, Layer

## KeyCombo

| field | type |
| --- | --- |
| modifiers | Vec<Modifier> |
| key | KeyName |

Refers to: Action

## JogSettings

| field | type |
| --- | --- |
| default_mode | JogMode (Scroll) |
| per_mode | Map<JogMode, JogBinding> (default Scroll: left Scroll(-1), right Scroll(+1)) |

Refers to: JogBinding, Config

## JogBinding

| field | type |
| --- | --- |
| left | Option<Action> |
| right | Option<Action> |
| threshold | u32 |
| shuttle_max_rate_hz | Option<u32> |

Refers to: Action, JogSettings

## OverrideSettings

| field | type |
| --- | --- |
| system_volume_step | f32 |
| app_volume_step | f32 |
| brightness_step | u8 |

Refers to: Config

## DeviceEvent (runtime)

| field | type |
| --- | --- |
| Press(Key) | string |
| Release(Key) | string |
| Jog{mode | JogMode, value: i32} |
| Battery{charging | bool, level: u8} |
| Connected | string |
| Disconnected | string |

Refers to: EngineState

## EngineState (runtime)

| field | type |
| --- | --- |
| layer_stack | Vec<LayerId> |
| held | Map<Key, KeyCombo> |
| latched | Set<KeyCombo> |
| jog_mode | JogMode |
| active_override | Option<OverrideMode> |
| off_by_resolve | bool |

Refers to: Layer, LedState, StatusEvent

## LedState (runtime)

| field | type |
| --- | --- |
| buttons | Set<Led> |
| jog | JogLed |

Refers to: EngineState

## StatusEvent (runtime)

| field | type |
| --- | --- |
| connection | Absent\|Connected\|Off\|AuthFailed |
| battery | Option<Battery> |
| layer | LayerId |
| override_errors | Vec<DisplayError\|AppVolumeStatus> |
| permissions | Map<Permission, Granted\|Missing> |

Refers to: EngineState

## MappingsExport (freewheel-mappings.json)

| field | type |
| --- | --- |
| version | u32 |
| layers | Vec<Layer> |
| base_layer | LayerId |
| jog | JogSettings |
| overrides | OverrideSettings |

Refers to: Config, Layer, JogSettings, OverrideSettings

```mermaid
%% freewheel data model (mermaid)
erDiagram
    Config {
        u32_1 version
        Settings settings
        Vec_Layer layers
        LayerId base_layer
        JogSettings jog
        OverrideSettings overrides
    }
    Settings {
        bool enabled
        bool_default_false start_at_login
        String_Resolve_exe_Resolve resolve_process_name
        u8_15 low_battery_threshold
    }
    Layer {
        LayerId id
        String name
        Map_Key_Action bindings
    }
    Action {
        KeyCombo Hotkey_combo
        KeyCombo Held_combo
        KeyCombo Latched_combo
        i32_P1 Scroll_amount
        String_args_Vec_String_enabled_bool LaunchApp_path
        LayerId LayerPush_target
        JogMode SetJogMode_mode
        OverrideMode OverrideToggle_mode
    }
    KeyCombo {
        Vec_Modifier modifiers
        KeyName key
    }
    JogSettings {
        JogMode_Scroll default_mode
        Map_JogMode_JogBinding_default_Scroll_left_Scroll_1_right_Scroll_1 per_mode
    }
    JogBinding {
        Option_Action left
        Option_Action right
        u32 threshold
        Option_u32 shuttle_max_rate_hz
    }
    OverrideSettings {
        f32 system_volume_step
        f32 app_volume_step
        u8 brightness_step
    }
    DeviceEvent_runtime {
        string Press_Key
        string Release_Key
        JogMode_value_i32 Jog_mode
        bool_level_u8 Battery_charging
        string Connected
        string Disconnected
    }
    EngineState_runtime {
        Vec_LayerId layer_stack
        Map_Key_KeyCombo held
        Set_KeyCombo latched
        JogMode jog_mode
        Option_OverrideMode active_override
        bool off_by_resolve
    }
    LedState_runtime {
        Set_Led buttons
        JogLed jog
    }
    StatusEvent_runtime {
        Absent_Connected_Off_AuthFailed connection
        Option_Battery battery
        LayerId layer
        Vec_DisplayError_AppVolumeStatus override_errors
        Map_Permission_Granted_Missing permissions
    }
    MappingsExport_freewheel_mappings_json {
        u32 version
        Vec_Layer layers
        LayerId base_layer
        JogSettings jog
        OverrideSettings overrides
    }
    Config }o--|| Settings : "refs"
    Config }o--|| Layer : "refs"
    Config }o--|| JogSettings : "refs"
    Config }o--|| OverrideSettings : "refs"
    Settings }o--|| Config : "refs"
    Layer }o--|| Config : "refs"
    Layer }o--|| Action : "refs"
    Action }o--|| KeyCombo : "refs"
    Action }o--|| Layer : "refs"
    KeyCombo }o--|| Action : "refs"
    JogSettings }o--|| JogBinding : "refs"
    JogSettings }o--|| Config : "refs"
    JogBinding }o--|| Action : "refs"
    JogBinding }o--|| JogSettings : "refs"
    OverrideSettings }o--|| Config : "refs"
    DeviceEvent_runtime }o--|| EngineState : "refs"
    EngineState_runtime }o--|| Layer : "refs"
    EngineState_runtime }o--|| LedState : "refs"
    EngineState_runtime }o--|| StatusEvent : "refs"
    LedState_runtime }o--|| EngineState : "refs"
    StatusEvent_runtime }o--|| EngineState : "refs"
    MappingsExport_freewheel_mappings_json }o--|| Config : "refs"
    MappingsExport_freewheel_mappings_json }o--|| Layer : "refs"
    MappingsExport_freewheel_mappings_json }o--|| JogSettings : "refs"
    MappingsExport_freewheel_mappings_json }o--|| OverrideSettings : "refs"
```
