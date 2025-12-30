<x-main-layout>
<h1>Events</h1>
<div>Now: {{ $now }}</div>
<div style="margin-top: 20px;">
    @foreach ($events as $event)
        <div style="display: grid; grid-template-columns: 200px 200px auto auto auto; gap: 10px; align-items: left; margin-bottom: 10px; border-bottom: 1px solid #ccc; padding-bottom: 10px;">
            <div>{{ $now->diffInMinutes($event->timestamp, ['short' => true]) }}</div>
            <div>{{$event->timestamp}}</div> 
            <x-system-event style="display: grid; grid-template-columns: 60px auto" :event="$event->event" />
        </div>
    @endforeach
</div>
</x-main-layout>
