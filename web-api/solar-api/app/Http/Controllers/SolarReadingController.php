<?php

namespace App\Http\Controllers;

use Bt\Solar\Upload;
use DateTimeImmutable;
use Bt\Solar\SystemEvent;
use App\Models\SolarReading;
use App\Models\Event;
use Illuminate\Http\Request;
use Illuminate\Support\Facades\Log;

class SolarReadingController extends Controller
{
    public function reading(Request $request)
    {
        $content = $request->getContent();
        $upload = new Upload();
        $upload->mergeFromString($content);
        $n = $upload->getEntries()->count();
        Log::info("Upload received ", ['entries' => $n]);
        $startTimestamp = $upload->getStartTimestamp();
        foreach ($upload->getEntries() as $entry) {
            $reading = $entry->getReading();
            $timestamp = (new DateTimeImmutable())->setTimestamp($startTimestamp + $entry->getOffsetInSeconds());
            $solarReading = new SolarReading;
            $factor = 1000.0;
            $solarReading->battery_voltage = $reading->getBatteryVoltage() / $factor;
            $solarReading->battery_current = $reading->getBatteryCurrent() / $factor;
            $solarReading->panel_voltage = $reading->getPanelVoltage() / $factor;
            $solarReading->panel_power = $reading->getPanelPower();
            $solarReading->load_current = $reading->getLoadCurrent()  / $factor;
            $solarReading->recorded_at = $timestamp;
            $solarReading->save();
        }
        return response('', 200)->withHeaders([]);
    }

    public function event(Request $request)
    {
        $content = $request->getContent();
        $event = new SystemEvent();
        $event->mergeFromString($content);
        $json = $event->serializeToJsonString();
        Log::info("Event received ", ['event' => $json]);
        $dbEvent = new Event;
        $dbEvent->timestamp = (new DateTimeImmutable())->setTimestamp($event->getTimestamp());
        $dbEvent->event = $json;
        $dbEvent->save();
        return response('', 200)->withHeaders([]); 
    }
}
