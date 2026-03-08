<?php

namespace App\Http\Controllers;

use App\Models\Event;
use App\Models\SolarReading;
use Illuminate\Http\Request;

class DashboardController extends Controller
{
    public function events(Request $request) {
        $events = Event::orderBy('timestamp','desc')->limit(20)->get();
        $now = now();
        return view('events', ['events' => $events, 'now' => $now]);
    }

    public function readings(Request $request) {
        $readings = SolarReading::orderBy('recorded_at','desc')->limit(36)->get();
        $now = now();
        return view('readings', ['readings' => $readings, 'now' => $now]);
    }
}
