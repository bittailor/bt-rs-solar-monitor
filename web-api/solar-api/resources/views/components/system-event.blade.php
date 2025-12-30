@switch($event->getEvent())
    @case('startup_event') 
        <x-startup-event :event="$event->getStartupEvent()" /> 
        @break
    @case('online_event') 
        <x-online-event :event="$event->getOnlineEvent()" /> 
        @break
    @case('offline_event') 
        <x-offline-event :event="$event->getOfflineEvent()" /> 
        @break
    @default 
        <div>Unknown event type '{{ $event->getEvent() }}'</div>
@endswitch        
