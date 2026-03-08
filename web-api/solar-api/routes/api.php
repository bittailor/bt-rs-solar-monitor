<?php

use Bt\Solar\Upload;
use Illuminate\Http\Request;
use Illuminate\Support\Facades\Route;
use Illuminate\Http\Middleware\HandleCors;
use App\Http\Middleware\StripToMinimalHeaders;
use App\Http\Controllers\SolarReadingController;
use App\Http\Middleware\ApiToken;

Route::get('/v2/info', function (Request $request) {
    return response('solar api v2', 200)->header('Content-Type', 'text/plain');
});

Route::middleware([ApiToken::class])->prefix('/v2/solar')->group(function () {
    Route::post('/reading', [SolarReadingController::class, 'reading'])->middleware([StripToMinimalHeaders::class]);
    Route::post('/event', [SolarReadingController::class, 'event'])->middleware([StripToMinimalHeaders::class]);
});



